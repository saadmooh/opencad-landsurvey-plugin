//! Daylight (slope) projection: cast a graded slope from a boundary station
//! until it meets the existing ground.
//!
//! Layer-C module (`std` only). The projection is a single-variable root
//! search along a unit horizontal ray. With `S` the unsigned run-per-rise
//! gradient ([`super::criteria::SlopeValue::as_gradient`], so `S = 2.0` for a
//! `2 : 1` face) the ray elevation is linear in horizontal distance `d`:
//!
//! ```text
//! z_ray(d) = z0 + d / S   (Cut  â€” the face rises from below existing ground)
//! z_ray(d) = z0 - d / S   (Fill â€” the face falls from above existing ground)
//! ```
//!
//! The daylight distance is the first `d >= 0` where `z_ray(d)` meets
//! `z_surface(d)`, found by marching on
//! [`GradingCriteria::sampling_interval`] and bisecting the sign change. The
//! result is therefore *within one sample interval* of the exact crossing, and
//! independent of the surface triangulation: sampling straddles a triangle
//! edge, the reported elevation comes from whichever facet contains the
//! crossing point, and the two agree on the shared edge.
//!
//! Only [`GradingTarget::Surface`] is supported here; the remaining targets
//! (elevation plane, fixed distance, arbitrary point) belong to later phases.

use crate::featureline::entity::{FeatureVertex, Point3d, ZSource};
use crate::featureline::entity::FeatureLine;
use crate::surface::{SpatialGrid, Surface};

use super::criteria::{GradingCriteria, GradingTarget};
use super::error::GradingError;
use super::project::{sample_feature_line, GradingSamplePoint, VertexKind};

// We'll add the project module use later in the function that needs it.

/// Plan-length below which a direction vector carries no usable heading.
const DEGENERATE_TOL: f64 = 1e-9;

/// Residual `|z_ray(0) - z_surface(0)|` at or below which the origin is
/// already on the surface: the daylight terminates immediately.
const Z_TOL: f64 = 1e-9;

/// Bisection halvings used to polish a bracketed crossing. The residual is
/// linear in `d` on every triangle, so this converges in one or two steps; the
/// relative tolerance below ends the loop early in ordinary cases.
const BISECTION_STEPS: usize = 64;

/// Relative bracket width that ends bisection early.
const HIT_TOL: f64 = 1e-12;

/// Ceiling on marched samples. A pathological `sampling_interval` (or a huge
/// `max_projection_distance`) would otherwise need an unbounded number of
/// steps to cross the search window, so the search reports
/// [`GradingError::RayLimitExceeded`] instead of spinning.
const MAX_MARCH_STEPS: usize = 4_000_000;

/// Which side of the graded plane the daylight face falls on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DaylightKind {
    /// Excavation: the graded plane is *below* existing ground, so the face
    /// rises from the boundary at `cut_slope` until it meets the surface.
    Cut,
    /// Embankment: the graded plane is *above* existing ground, so the face
    /// descends from the boundary at `fill_slope` until it meets the surface.
    Fill,
}

impl DaylightKind {
    /// Sign of the ray's `dz/dd` (`+1` rising for cut, `-1` falling for fill).
    fn sign(self) -> f64 {
        match self {
            DaylightKind::Cut => 1.0,
            DaylightKind::Fill => -1.0,
        }
    }

    /// The `cut_slope` / `fill_slope` field this kind projects with.
    fn slope_field(self) -> &'static str {
        match self {
            DaylightKind::Cut => "cut_slope",
            DaylightKind::Fill => "fill_slope",
        }
    }

    /// Unsigned run-per-rise gradient, positive and finite or the projection
    /// is meaningless: a vertical face (`h -> 0`) or a `0`/`inf` divisor
    /// cannot be walked.
    fn gradient(self, criteria: &GradingCriteria) -> f64 {
        match self {
            DaylightKind::Cut => criteria.cut_slope.as_gradient(),
            DaylightKind::Fill => criteria.fill_slope.as_gradient(),
        }
    }
}

impl core::fmt::Display for DaylightKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            DaylightKind::Cut => "cut",
            DaylightKind::Fill => "fill",
        })
    }
}

/// One successful daylight projection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DaylightResult {
    /// Station the slope left from, echoed from the call.
    pub origin: [f64; 3],
    /// Unit horizontal direction actually marched (the input normalized).
    pub direction: [f64; 2],
    /// Intersection point on the existing surface; `z` is the projected slope
    /// elevation, which equals the surface elevation to within the bisection
    /// tolerance.
    pub hit: [f64; 3],
    /// Horizontal distance from `origin` to `hit`.
    pub distance: f64,
    /// Which slope the ray followed.
    pub kind: DaylightKind,
    /// Samples taken while marching: `0` for an immediate hit, otherwise the
    /// 1-based index of the bracketing sample.
    pub iterations: usize,
}

/// A horizontal ray carrying a linear slope: `z(d) = z0 + m * d`.
#[derive(Debug, Clone, Copy)]
struct Ray {
    o: [f64; 2],
    d: [f64; 2],
    z0: f64,
    /// Signed `dz/dd` â€” `+1/S` for cut, `-1/S` for fill.
    m: f64,
}

impl Ray {
    /// Ray elevation at horizontal distance `d`.
    fn z_at(&self, d: f64) -> Result<f64, GradingError> {
        let z = self.z0 + self.m * d;
        if z.is_finite() {
            Ok(z)
        } else {
            Err(GradingError::CalculationOverflow)
        }
    }

    /// Ray point at horizontal distance `d`.
    fn point(&self, d: f64) -> Result<[f64; 3], GradingError> {
        let p = [
            self.o[0] + self.d[0] * d,
            self.o[1] + self.d[1] * d,
            self.z_at(d)?,
        ];
        if p[0].is_finite() && p[1].is_finite() {
            Ok(p)
        } else {
            Err(GradingError::CalculationOverflow)
        }
    }

    /// Signed residual `z_ray(d) - z_surface(d)`.
    ///
    /// Leaving the mesh is [`GradingError::RayTargetNotFound`] rather than a
    /// miss: there is no surface left to daylight onto.
    fn delta(&self, d: f64, surface: &Surface, grid: &SpatialGrid) -> Result<f64, GradingError> {
        let p = self.point(d)?;
        match surface.interpolate_z_fast(grid, p[0], p[1]) {
            Some(sz) => {
                let f = p[2] - sz;
                if f.is_finite() {
                    Ok(f)
                } else {
                    Err(GradingError::CalculationOverflow)
                }
            }
            None => Err(GradingError::RayTargetNotFound),
        }
    }
}

/// Project a cut or fill slope from `origin` along `direction` until it meets
/// * `origin` â€” boundary station `[x, y, z]` the daylight starts from.
/// * `direction` â€” horizontal heading; normalized internally, so any positive
///   multiple of a unit vector gives the same projection.
///   * `criteria` â€” supplies the slope, the march interval, and the distance cap.
/// * `surface` â€” the existing ground surface.
///
/// # Errors
///
/// * [`GradingError::InvalidCriteria`] â€” `criteria.validate()` rejects a
///   field; the target is not [`GradingTarget::Surface`]; or the selected
///   slope does not yield a finite positive gradient (a vertical face, or a
///   `Ratio` with a zero vertical component).
/// * [`GradingError::DegenerateSegment`] â€” `direction` is shorter than
///   `1e-9`, so it carries no heading.
/// * [`GradingError::CalculationOverflow`] â€” a coordinate is non-finite, or
///   the marched position leaves the representable range.
/// * [`GradingError::RayTargetNotFound`] â€” the ray leaves the surface without
///   meeting it (including a direction pointing straight out of the mesh).
/// * [`GradingError::RayLimitExceeded`] â€” `max_projection_distance` was
///   reached with no crossing, or resolving that distance at
///   `sampling_interval` would need more than `4_000_000` samples.
///
/// # Notes
///
/// * An origin already on the surface within `1e-9` returns immediately with
///   `distance == 0.0` and `iterations == 0`.
/// * The crossing is bracketed on `sampling_interval`, so `distance` is within
///   one interval of the true intersection; bisection then refines it to a
///   relative bracket of `1e-12`.
pub fn daylight(
    origin: [f64; 3],
    direction: [f64; 2],
    kind: DaylightKind,
    surface: &Surface,
    grid: &SpatialGrid,
    criteria: &GradingCriteria,
) -> Result<DaylightResult, GradingError> {
    criteria.validate()?;

    if criteria.target != GradingTarget::Surface {
        return Err(GradingError::InvalidCriteria(
            "daylight supports only GradingTarget::Surface".to_string(),
        ));
    }

    let gradient = kind.gradient(criteria);
    if !gradient.is_finite() || gradient <= 0.0 {
        return Err(GradingError::InvalidCriteria(format!(
            "{} must yield a finite positive run-per-rise gradient",
            kind.slope_field()
        )));
    }

    if !origin[0].is_finite()
        || !origin[1].is_finite()
        || !origin[2].is_finite()
        || !direction[0].is_finite()
        || !direction[1].is_finite()
    {
        return Err(GradingError::CalculationOverflow);
    }
    let len = direction[0].hypot(direction[1]);
    if len < DEGENERATE_TOL {
        return Err(GradingError::DegenerateSegment);
    }

    let ray = Ray {
        o: [origin[0], origin[1]],
        d: [direction[0] / len, direction[1] / len],
        z0: origin[2],
        m: kind.sign() / gradient,
    };

    let f0 = ray.delta(0.0, surface, grid)?;
    if f0.abs() <= Z_TOL {
        return Ok(DaylightResult {
            origin,
            direction: ray.d,
            hit: [ray.o[0], ray.o[1], ray.z_at(0.0)?],
            distance: 0.0,
            kind,
            iterations: 0,
        });
    }

    let step = criteria.sampling_interval;
    let limit = criteria.max_projection_distance;
    let needed = limit / step;
    if !needed.is_finite() || needed > MAX_MARCH_STEPS as f64 {
        return Err(GradingError::RayLimitExceeded);
    }

    let mut prev_d = 0.0;
    let mut prev_f = f0;
    for n in 1..=MAX_MARCH_STEPS {
        let raw = n as f64 * step;
        if !raw.is_finite() {
            return Err(GradingError::CalculationOverflow);
        }
        // Never sample past the cap: the final station is the cap itself, so a
        // crossing inside the window is still found and one outside it is
        // reported as a limit failure rather than a late success.
        let capped = raw > limit;
        let d = if capped { limit } else { raw };
        let f = ray.delta(d, surface, grid)?;
        if f == 0.0 || (prev_f > 0.0) != (f > 0.0) {
            let (distance, z) = refine_crossing(&ray, prev_d, d, prev_f, f, surface, grid)?;
            let p = ray.point(distance)?;
            return Ok(DaylightResult {
                origin,
                direction: ray.d,
                hit: [p[0], p[1], z],
                distance,
                kind,
                iterations: n,
            });
        }
        if capped {
            return Err(GradingError::RayLimitExceeded);
        }
        prev_d = d;
        prev_f = f;
    }
    Err(GradingError::RayLimitExceeded)
}

/// Bisect a bracketed sign change down to a relative bracket of `HIT_TOL`.
///
/// `flo` is the residual at `lo` and `fhi` the residual at `hi`; the caller has
/// already established that the crossing lies between them.
fn refine_crossing(
    ray: &Ray,
    lo: f64,
    hi: f64,
    flo: f64,
    fhi: f64,
    surface: &Surface,
    grid: &SpatialGrid,
) -> Result<(f64, f64), GradingError> {
    if fhi == 0.0 {
        return Ok((hi, ray.z_at(hi)?));
    }
    let (mut lo, mut hi, mut flo, mut fhi) = (lo, hi, flo, fhi);
    for _ in 0..BISECTION_STEPS {
        let mid = lo + 0.5 * (hi - lo);
        let fm = ray.delta(mid, surface, grid)?;
        if fm == 0.0 {
            return Ok((mid, ray.z_at(mid)?));
        }
        if (fm > 0.0) == (flo > 0.0) {
            lo = mid;
            flo = fm;
        } else {
            hi = mid;
            fhi = fm;
        }
        if hi - lo <= HIT_TOL * hi.abs().max(1.0) {
            break;
        }
    }
    let d = lo + 0.5 * (hi - lo);
    Ok((d, ray.z_at(d)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grading::criteria::GradingCriteria;
    use crate::surface::Triangle;

    /// Helper to create a simple triangular surface for testing.
    fn make_test_surface() -> Surface {
        // Create a simple flat surface at z=0
        let mut triangles = Vec::new();
        // Two triangles forming a square quad from (-10,-10,0) to (10,10,0)
        triangles.push(Triangle::new([-10.0, -10.0, 0.0], [10.0, -10.0, 0.0], [10.0, 10.0, 0.0]));
        triangles.push(Triangle::new([-10.0, -10.0, 0.0], [10.0, 10.0, 0.0], [-10.0, 10.0, 0.0]));
        Surface::new(triangles)
    }

    #[test]
    fn test_daylight_origin_on_surface() {
        let surface = make_test_surface();
        let grid = SpatialGrid::build(&surface, 0.0);
        let criteria = GradingCriteria::default();
        
        // Origin on the surface
        let origin = [0.0, 0.0, 0.0];
        let direction = [1.0, 0.0]; // East
        
        let result = daylight(origin, direction, DaylightKind::Cut, &surface, &grid, &criteria).unwrap();
        
        assert_eq!(result.distance, 0.0);
        assert_eq!(result.iterations, 0);
        assert_eq!(result.hit, [0.0, 0.0, 0.0]);
        assert_eq!(result.kind, DaylightKind::Cut);
    }

    #[test]
    fn test_daylight_simple_cut() {
        let surface = make_test_surface();
        let grid = SpatialGrid::build(&surface, 0.0);
        let mut criteria = GradingCriteria::default();
        criteria.cut_slope = crate::grading::criteria::SlopeValue::Ratio(2, 1); // 2:1 slope
        
        // Origin at [0, 0, -5] (5 units below surface)
        let origin = [0.0, 0.0, -5.0];
        let direction = [1.0, 0.0]; // East
        
        let result = daylight(origin, direction, DaylightKind::Cut, &surface, &grid, &criteria).unwrap();
        
        // For a 2:1 cut slope, to rise 5 units we need to go 10 units horizontally
        // Expected hit point: [10.0, 0.0, 0.0]
        assert!((result.distance - 10.0).abs() < 0.1);
        assert!((result.hit[0] - 10.0).abs() < 0.1);
        assert!((result.hit[1] - 0.0).abs() < 0.1);
        assert!((result.hit[2] - 0.0).abs() < 0.1);
        assert_eq!(result.kind, DaylightKind::Cut);
    }

    #[test]
    fn test_daylight_simple_fill() {
        let surface = make_test_surface();
        let grid = SpatialGrid::build(&surface, 0.0);
        let mut criteria = GradingCriteria::default();
        criteria.fill_slope = crate::grading::criteria::SlopeValue::Ratio(2, 1); // 2:1 slope
        
        // Origin at [0, 0, 5] (5 units above surface)
        let origin = [0.0, 0.0, 5.0];
        let direction = [1.0, 0.0]; // East
        
        let result = daylight(origin, direction, DaylightKind::Fill, &surface, &grid, &criteria).unwrap();
        
        // For a 2:1 fill slope, to drop 5 units we need to go 10 units horizontally
        // Expected hit point: [10.0, 0.0, 0.0]
        assert!((result.distance - 10.0).abs() < 0.1);
        assert!((result.hit[0] - 10.0).abs() < 0.1);
        assert!((result.hit[1] - 0.0).abs() < 0.1);
        assert!((result.hit[2] - 0.0).abs() < 0.1);
        assert_eq!(result.kind, DaylightKind::Fill);
    }
}

// New functions for Phase 2: Ray-Casting & Daylight Projection

/// Cast a single daylight ray using a pre-built spatial grid.
pub fn cast_daylight_ray(
    origin: [f64; 3],
    direction_2d: [f64; 2],
    criteria: &GradingCriteria,
    surface: &Surface,
    grid: &SpatialGrid,
) -> Result<[f64; 3], GradingError> {
    // Validate criteria.
    criteria.validate()?;

    // Determine if the origin is in cut or fill by comparing its Z to the surface Z.
    let ground_z = surface
        .interpolate_z_fast(grid, origin[0], origin[1])
        .ok_or(GradingError::RayTargetNotFound)?; // If we can't get ground Z, we can't decide cut/fill.

    let kind = if origin[2] < ground_z {
        DaylightKind::Cut
    } else {
        DaylightKind::Fill
    };

    // Call the existing daylight function.
    let result = daylight(
        origin,
        direction_2d,
        kind,
        surface,
        grid,
        criteria,
    )?;

Ok(result.hit)
}

/// Generate a daylight line by projecting the feature line onto the surface.
///
/// This function samples the feature line according to the grading criteria,
/// then for each sampled point (including radial fans at exterior convex
/// corners) casts a daylight ray in the direction of the outward normal (or
/// corner bisector) and collects the intersection points into a new feature
/// line named `{original_name}_Daylight`.
///
/// # Errors
///
/// Propagates errors from [`sample_feature_line`], [`cast_daylight_ray`], and
/// radial fan generation.
pub fn generate_daylight_line(
    fl: &FeatureLine,
    criteria: &GradingCriteria,
    surface: &Surface,
) -> Result<FeatureLine, GradingError> {

    use super::project::{sample_feature_line, GradingSamplePoint, VertexKind};

    // Sample the feature line to get points with outward normals and vertex kinds.
    let samples = sample_feature_line(fl, criteria)?;

    // Build a spatial grid for efficient surface queries (will be reused for all rays)
    let grid = SpatialGrid::build(surface, 0.0);

    // We'll collect the daylight points (intersection points) in order.
    let mut daylight_points: Vec<[f64; 3]> = Vec::new();

    // First pass: collect all points and identify exterior convex corners for radial fan processing
    let mut exterior_convex_corners: Vec<usize> = Vec::new();
    
    for (i, sample) in samples.iter().enumerate() {
        if sample.vertex_kind == VertexKind::ExteriorConvex {
            exterior_convex_corners.push(i);
        }
    }

// Process each sample point
    for (i, sample) in samples.iter().enumerate() {
        match sample.vertex_kind {
            VertexKind::ExteriorConvex => {
                // Determine incoming normal (from previous edge) and outgoing normal (current edge)
                let prev_idx = if i == 0 {
                    samples.len().saturating_sub(2)
                } else {
                    i - 1
                };
                let next_idx = if i + 1 >= samples.len() {
                    1
                } else {
                    i + 1
                };

                let n_in = samples[prev_idx].outward_normal;
                let n_out = samples[next_idx].outward_normal;

                let theta_in = n_in[1].atan2(n_in[0]);
                let mut theta_out = n_out[1].atan2(n_out[0]);

                // Ensure clockwise sweep (decreasing angle)
                while theta_out > theta_in {
                    theta_out -= core::f64::consts::TAU;
                }

                let sweep = (theta_in - theta_out).abs();
                let max_fan_step = 15.0_f64.to_radians(); // 15 degrees
                let num_steps = ((sweep / max_fan_step).ceil() as usize).max(2);

                for step in 0..=num_steps {
                    let frac = step as f64 / num_steps as f64;
                    let theta = theta_in - frac * sweep;
                    let dir = [theta.cos(), theta.sin()];

                    let hit = cast_daylight_ray(sample.xyz, dir, criteria, surface, &grid)?;
                    daylight_points.push(hit);
                }
            }
            VertexKind::Standard | VertexKind::Interior => {
                let hit = cast_daylight_ray(
                    sample.xyz,
                    sample.outward_normal,
                    criteria,
                    surface,
                    &grid,
                )?;
                daylight_points.push(hit);
            }
        }
    }

    // Now we need to create a new FeatureLine from the daylight points.
    // We'll assume the points are in order and we want to create a feature line
    // with linear segments (no bulge) and default style.

    let mut daylight_fl = FeatureLine::new(format!("{}_Daylight", fl.name));
    for pt in daylight_points {
        daylight_fl.push_vertex(FeatureVertex::new_2d(pt[0], pt[1], pt[2]));
    }

    Ok(daylight_fl)
}