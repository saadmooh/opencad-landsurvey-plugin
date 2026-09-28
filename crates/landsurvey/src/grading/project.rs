//! Feature line sampling and densification for grading.
//!
//! Layer-C module (`std` only). Samples a feature line at intervals not exceeding
//! the grading criteria's sampling interval, densifies bulge arcs, and computes
//! outward normals and corner types for each sampled point.

use crate::featureline::entity::FeatureLine;
use crate::grading::criteria::GradingCriteria;
use crate::grading::error::GradingError;
use crate::grading::math::{densify_bulge, outward_normal_2d, vertex_bisector_2d, orient2d};

/// Kind of vertex in the sampled polyline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VertexKind {
    /// Point lies on a straight edge (or is an endpoint of an open feature line).
    Standard,
    /// Exterior convex corner (turns right for a clockwise-wound boundary).
    ExteriorConvex,
    /// Interior concave corner (turns left for a clockwise-wound boundary).
    Interior,
}

/// A sampled point along a feature line with grading-relevant attributes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradingSamplePoint {
    /// 3D coordinates (X, Y, Z) of the sampled point.
    pub xyz: [f64; 3],
    /// Outward 2D unit normal vector (or corner bisector at vertices).
    pub outward_normal: [f64; 2],
    /// Classification of the point as a corner type or standard edge point.
    pub vertex_kind: VertexKind,
}

/// Samples a feature line according to the given grading criteria.
///
/// For each segment, if its bulge is non-zero, the arc is densified using
/// `densify_bulge` with `criteria.max_sagitta`. The resulting polyline (linear
/// and densified segments) is then subdivided so that no segment exceeds
/// `criteria.sampling_interval`. For each resulting vertex, the function
/// records the 3D coordinates, the outward 2D unit normal (or corner bisector
/// at vertices), and whether the vertex is an exterior convex corner, interior
/// corner, or standard edge point.
///
/// # Errors
///
/// Propagates errors from [`densify_bulge`], [`outward_normal_2d`], and
/// [`vertex_bisector_2d`] (which in turn may return
/// [`GradingError::DegenerateSegment`] or [`GradingError::CalculationOverflow`]).
pub fn sample_feature_line(
    fl: &FeatureLine,
    criteria: &GradingCriteria,
) -> Result<Vec<GradingSamplePoint>, GradingError> {
    // Early exit for degenerate feature lines.
    if fl.vertices.len() < 2 {
        return Ok(vec![]);
    }

    // Step 1: Build a polyline with bulge densification and track original vertices.
    let mut all_pts: Vec<[f64; 3]> = Vec::new();
    let mut orig_flags: Vec<Option<usize>> = Vec::new(); // Same length as all_pts.

    for seg_idx in 0..fl.vertices.len() - 1 {
        let start_vtx = &fl.vertices[seg_idx];
        let end_vtx = &fl.vertices[seg_idx + 1];

        let p1 = [start_vtx.pt.x, start_vtx.pt.y, start_vtx.pt.z];
        let p2 = [end_vtx.pt.x, end_vtx.pt.y, end_vtx.pt.z];
        let bulge = start_vtx.bulge;

        // Obtain points for this segment after bulge densification.
        let seg_pts = if bulge.abs() > super::math::STRAIGHT_BULGE_TOL {
            densify_bulge(p1, p2, bulge, criteria.max_sagitta)?
        } else {
            vec![p1, p2]
        };

        // Append seg_pts to all_pts, avoiding duplication of the shared vertex.
        if seg_idx == 0 {
            // First segment: add all points.
            for (i, pt) in seg_pts.iter().enumerate() {
                all_pts.push(*pt);
                // The first point of the first segment corresponds to original vertex 0.
                // The last point of the segment corresponds to original vertex seg_idx+1.
                // Interior points are not original.
let flag = if i == 0 {
                        Some(0)
                    } else if i == seg_pts.len() - 1 {
                        Some(seg_idx + 1)
                    } else {
                        None
                    };
                    orig_flags.push(flag);
            }
        } else {
            // Subsequent segments: skip the first point (duplicate of previous segment's last point).
            for (i, pt) in seg_pts.iter().enumerate().skip(1) {
                all_pts.push(*pt);
                // The first point we skip is the same as the last point of the previous segment,
                // which was already marked as the end vertex of the previous segment.
                // So we only need to mark the last point of this segment as an original vertex.
                let flag = if i == seg_pts.len() - 1 {
                    Some(seg_idx + 1)
                } else {
                    None
                };
                orig_flags.push(flag);
            }
        }
    }

    // Step 2: Subdivide the polyline so that no segment exceeds the sampling interval.
    let max_len = criteria.sampling_interval;
    let mut final_pts: Vec<[f64; 3]> = Vec::new();
    let mut final_orig_flags: Vec<Option<usize>> = Vec::new();

    if all_pts.is_empty() {
        return Ok(vec![]);
    }

    // We'll iterate over segments of all_pts.
    for i in 0..all_pts.len() - 1 {
        let a = all_pts[i];
        let b = all_pts[i + 1];
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let dz = b[2] - a[2];
        let dist = dx.hypot(dy).hypot(dz);

        // Always add the start point of the segment.
        final_pts.push(a);
        final_orig_flags.push(orig_flags[i]);

        if dist > max_len {
            // Split the segment into n equal parts.
            let n = (dist / max_len).ceil() as usize;
            let step = 1.0 / n as f64;
            // Generate points from a to b, excluding a (since we already added it).
            for k in 1..n {
                let t = k as f64 * step;
                let interp = [
                    a[0] + dx * t,
                    a[1] + dy * t,
                    a[2] + dz * t,
                ];
                final_pts.push(interp);
                // These interpolated points are not original vertices.
                final_orig_flags.push(None);
            }
        }
        // If dist <= max_len, we do nothing extra; the end point will be added as the start of the next segment.
        // We will handle the last point after the loop.
    }

    // Add the last point of the polyline.
    final_pts.push(all_pts[all_pts.len() - 1]);
    final_orig_flags.push(orig_flags[orig_flags.len() - 1]);

    // Step 3: Compute the sampled points with normals and vertex kinds.
    let mut samples: Vec<GradingSamplePoint> = Vec::with_capacity(final_pts.len());

    for (i, pt) in final_pts.iter().enumerate() {
        let xyz = *pt;
        let outward_normal = if i > 0 && i < final_pts.len() - 1 {
            // Interior point: compute bisector of outward normals of incoming and outgoing segments.
            let prev = &final_pts[i - 1];
            let next = &final_pts[i + 1];
            // Use only XY components for the 2D normals.
            let prev_xy = [prev[0], prev[1]];
            let curr_xy = [xyz[0], xyz[1]];
            let next_xy = [next[0], next[1]];
            vertex_bisector_2d(prev_xy, curr_xy, next_xy)?
        } else if i == 0 && final_pts.len() > 1 {
            // First point: use outward normal of the first segment.
            let curr_xy = [xyz[0], xyz[1]];
            let next_xy = [final_pts[1][0], final_pts[1][1]];
            outward_normal_2d(curr_xy, next_xy)?
        } else if i == final_pts.len() - 1 && final_pts.len() > 1 {
            // Last point: use outward normal of the last segment.
            let prev_xy = [final_pts[i - 1][0], final_pts[i - 1][1]];
            let curr_xy = [xyz[0], xyz[1]];
            outward_normal_2d(prev_xy, curr_xy)?
        } else {
            // Degenerate case: single point polyline (should not happen due to early exit).
            [0.0, 0.0]
        };

        let vertex_kind = if let Some(orig_idx) = final_orig_flags[i] {
            // This point corresponds to an original feature line vertex.
            if orig_idx == 0 || orig_idx == fl.vertices.len() - 1 {
                // Endpoint of an open feature line: treat as standard.
                VertexKind::Standard
            } else {
                // Check the turn at this original vertex.
                let prev_vtx = &fl.vertices[orig_idx - 1];
                let curr_vtx = &fl.vertices[orig_idx];
                let next_vtx = &fl.vertices[orig_idx + 1];
                let o = orient2d(
                    [prev_vtx.pt.x, prev_vtx.pt.y],
                    [curr_vtx.pt.x, curr_vtx.pt.y],
                    [next_vtx.pt.x, next_vtx.pt.y],
                );
                if o.is_finite() && o < 0.0 {
                    VertexKind::ExteriorConvex
                } else if o.is_finite() && o > 0.0 {
                    VertexKind::Interior
                } else {
                    VertexKind::Standard
                }
            }
        } else {
            // Interpolated point: not an original vertex.
            VertexKind::Standard
        };

        samples.push(GradingSamplePoint {
            xyz,
            outward_normal,
            vertex_kind,
        });
    }

    Ok(samples)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::featureline::entity::{FeatureVertex, Point3d, ZSource};
    use crate::grading::criteria::GradingCriteria;

    /// Helper to create a feature line from a list of points (x, y, z) with zero bulge.
    fn make_fl(points: &[(f64, f64, f64)]) -> FeatureLine {
        let mut fl = FeatureLine::new("test".to_string());
        for (x, y, z) in points {
            fl.push_vertex(FeatureVertex::new_2d(*x, *y, *z));
        }
        fl
    }

    #[test]
    fn samples_straight_line() {
        let fl = make_fl(&[(0.0, 0.0, 0.0), (10.0, 0.0, 5.0)]);
        let criteria = GradingCriteria::default();
        let samples = sample_feature_line(&fl, &criteria).unwrap();
        // With default sampling_interval = 1.0, we expect points at 0,1,2,...,10 m.
        // That's 11 points.
        assert_eq!(samples.len(), 11);
        // Check first and last points.
        assert_eq!(samples[0].xyz, [0.0, 0.0, 0.0]);
        assert_eq!(samples[10].xyz, [10.0, 0.0, 5.0]);
        // Outward normal for a horizontal segment (eastward) should be [0.0, -1.0] (south).
        // Because the boundary is wound clockwise, the outward side is to the right.
        // For a segment from (0,0) to (10,0), the direction is east, outward is south.
        assert_eq!(samples[0].outward_normal, [0.0, -1.0]);
        assert_eq!(samples[10].outward_normal, [0.0, -1.0]);
        // All points should be standard edge points (no corners).
        for s in &samples {
            assert_eq!(s.vertex_kind, VertexKind::Standard);
        }
    }

    #[test]
    fn samples_bulge_arc() {
        // Create a quarter arc from (1,0,0) to (0,1,0) with bulge = tan(22.5°).
        let bulge = (std::f64::consts::PI / 8.0).tan();
        let mut fl = FeatureLine::new("arc".to_string());
        fl.push_vertex(FeatureVertex::new(Point3d::new(1.0, 0.0, 0.0), bulge, ZSource::TINInterpolated));
        fl.push_vertex(FeatureVertex::new_2d(0.0, 1.0, 0.0));
        let criteria = GradingCriteria::default();
        let samples = sample_feature_line(&fl, &criteria).unwrap();
        // We expect at least the start, end, and some interior points.
        assert!(samples.len() >= 3);
        assert_eq!(samples[0].xyz, [1.0, 0.0, 0.0]);
        let last_idx = samples.len() - 1;
        assert_eq!(samples[last_idx].xyz, [0.0, 1.0, 0.0]);
        // The vertex at the start and end are not corners because the feature line has only two vertices.
        // So they should be standard edge points.
        assert_eq!(samples[0].vertex_kind, VertexKind::Standard);
        assert_eq!(samples[last_idx].vertex_kind, VertexKind::Standard);
        // The outward normals should vary along the arc.
        // We'll just check that they are unit vectors.
        for s in &samples {
            let len = s.outward_normal[0].hypot(s.outward_normal[1]);
            assert!((len - 1.0).abs() < 1e-9, "non-unit normal: {:?}", s.outward_normal);
        }
    }

    #[test]
    fn corner_detection() {
        // Create an L-shape: (0,0,0) -> (10,0,0) -> (10,10,0). This is a right turn (convex).
        let fl = make_fl(&[(0.0, 0.0, 0.0), (10.0, 0.0, 0.0), (10.0, 10.0, 0.0)]);
        let criteria = GradingCriteria::default();
        let samples = sample_feature_line(&fl, &criteria).unwrap();
        // We expect samples along each leg. The corner point should be at (10,0,0).
        // We'll find the sample closest to (10,0,0).
        let mut corner_sample = None;
        for s in &samples {
            if (s.xyz[0] - 10.0).abs() < 0.5 && (s.xyz[1] - 0.0).abs() < 0.5 {
                corner_sample = Some(s);
                break;
            }
        }
        let corner_sample = corner_sample.expect("corner sample not found");
        // The corner sample should be classified as exterior convex.
        assert_eq!(corner_sample.vertex_kind, VertexKind::ExteriorConvex);
        // The outward normal at a convex corner should point outward from the L-shape.
        // For the L-shape with clockwise winding, the outward normal at the corner
        // should point away from the interior, i.e., to the southeast? Let's compute:
        // Incoming segment: from (0,0) to (10,0) -> eastward, outward normal = south [0,-1].
        // Outgoing segment: from (10,0) to (10,10) -> northward, outward normal = east [1,0].
        // The bisector of [0,-1] and [1,0] should be [1/sqrt(2), -1/sqrt(2)].
        // That points to the southeast, which is outward from the L-shape (the interior is to the
        // northwest). So we expect the outward normal to be approximately [0.707, -0.707].
        let nx = corner_sample.outward_normal[0];
        let ny = corner_sample.outward_normal[1];
        let expected = 1.0 / 2.0f64.sqrt();
        assert!((nx - expected).abs() < 0.01, "nx {} vs {}", nx, expected);
        assert!((ny + expected).abs() < 0.01, "ny {} vs {}", ny, -expected);
    }

    #[test]
    fn interior_corner_detection() {
        // Create a reverse L-shape: (0,0,0) -> (0,10,0) -> (10,10,0). This is a left turn (concave).
        let fl = make_fl(&[(0.0, 0.0, 0.0), (0.0, 10.0, 0.0), (10.0, 10.0, 0.0)]);
        let criteria = GradingCriteria::default();
        let samples = sample_feature_line(&fl, &criteria).unwrap();
        // Find the sample near the corner (0,10,0).
        let mut corner_sample = None;
        for s in &samples {
            if (s.xyz[0] - 0.0).abs() < 0.5 && (s.xyz[1] - 10.0).abs() < 0.5 {
                corner_sample = Some(s);
                break;
            }
        }
        let corner_sample = corner_sample.expect("corner sample not found");
        // The corner sample should be classified as interior.
        assert_eq!(corner_sample.vertex_kind, VertexKind::Interior);
    }

    #[test]
    fn respects_sampling_interval() {
        // A long straight line: 0 to 100 on x, y=0, z=0.
        let fl = make_fl(&[(0.0, 0.0, 0.0), (100.0, 0.0, 0.0)]);
        let mut criteria = GradingCriteria::default();
        criteria.sampling_interval = 5.0; // 5 m
        let samples = sample_feature_line(&fl, &criteria).unwrap();
        // We expect points at 0,5,10,...,100 -> 21 points.
        assert_eq!(samples.len(), 21);
        // Check that the distance between consecutive samples is <= 5.0 (with a little tolerance).
        for i in 0..samples.len() - 1 {
            let dx = samples[i + 1].xyz[0] - samples[i].xyz[0];
            let dy = samples[i + 1].xyz[1] - samples[i].xyz[1];
            let dz = samples[i + 1].xyz[2] - samples[i].xyz[2];
            let dist = dx.hypot(dy).hypot(dz);
            assert!(dist <= criteria.sampling_interval + 1e-9, "segment too long: {} > {}", dist, criteria.sampling_interval);
        }
    }

    #[test]
    fn no_duplicate_vertices_after_sampling() {
        // A long straight line: 0 to 100 on x, y=0, z=0.
        let fl = make_fl(&[(0.0, 0.0, 0.0), (100.0, 0.0, 0.0)]);
        let mut criteria = GradingCriteria::default();
        criteria.sampling_interval = 1.0; // 1 m
        let samples = sample_feature_line(&fl, &criteria).unwrap();
        // We expect points at 0,1,2,...,100 -> 101 points.
        assert_eq!(samples.len(), 101);
        // Check that no two consecutive points are the same (within tolerance).
        for i in 0..samples.len() - 2 {
            let p1 = samples[i].xyz;
            let p2 = samples[i + 1].xyz;
            let dx = p1[0] - p2[0];
            let dy = p1[1] - p2[1];
            let dz = p1[2] - p2[2];
            let dist = dx.hypot(dy).hypot(dz);
            assert!(dist > 1e-9, "duplicate points at index {} and {}", i, i + 1);
        }
    }
}

