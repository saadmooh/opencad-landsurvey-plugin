//! Low-level grading math: bulge-arc densification, segment normals, and
//! vertex angle bisectors.
//!
//! Pure functions, `std` only. All angles are in radians; bulge follows the
//! CAD convention `bulge = tan(sweep / 4)`, positive for counter-clockwise
//! arcs (as stored on [`crate::featureline::FeatureVertex`]).
//!
//! **Orientation convention:** grading boundaries wind clockwise, so the
//! *outward* side of a directed segment is its right-hand normal
//! ([`outward_normal_2d`]) and convex corners turn right
//! ([`is_convex_corner`]).

use super::error::GradingError;

/// Tolerance below which a bulge is treated as a straight segment.
pub const STRAIGHT_BULGE_TOL: f64 = 1e-9;

/// Length below which a chord — or a vector whose normal is requested — is
/// treated as degenerate.
const DEGENERATE_TOL: f64 = 1e-9;

/// Length below which a summed bisector collapses to its fallback direction.
const BISECTOR_TOL: f64 = 1e-9;

/// Angular step used when the sagitta tolerance is unusable (`<= 0` or
/// non-finite): 22.5°, i.e. at most 16 segments per full turn.
const FALLBACK_STEP: f64 = std::f64::consts::PI / 8.0;

/// Hard ceiling on arc segments. It bounds allocation for pathological
/// radius/sagitta ratios; beyond it the sagitta bound is relaxed rather than
/// exhausting memory or saturating into `usize::MAX`.
const MAX_SEGMENTS: usize = 1_000_000;

/// Densify a bulge arc into straight segments bounded by a sagitta tolerance.
///
/// * `p1`, `p2` — chord endpoints as `[x, y, z]`.
/// * `bulge` — `tan(sweep / 4)`; `|bulge| < 1e-9` is a straight segment.
/// * `max_sagitta` — maximum allowed chord-to-arc deviation in world units.
///   Non-positive or non-finite values select the fixed 22.5° fallback step.
///
/// # Errors
///
/// * [`GradingError::CalculationOverflow`] — a coordinate, the bulge, or a
///   derived radius/center is non-finite.
/// * [`GradingError::DegenerateSegment`] — the chord is shorter than
///   `DEGENERATE_TOL`. This is checked **before** the straight-segment
///   shortcut, so a zero-length segment is rejected whatever its bulge is.
///
/// # Returns
///
/// * `Ok(vec![p1, p2])` for a straight segment (`|bulge| < 1e-9`).
/// * Otherwise at least three points (`>= 2` arc segments): the first is
///   bit-exactly `p1`, the last bit-exactly `p2`, and the count never exceeds
///   `MAX_SEGMENTS` points plus one.
///
/// Elevation interpolates linearly in normalized arc length (`t = i / n_segs`),
/// i.e. uniformly in sweep angle for a circular arc, so z is monotonic
/// whenever `p1[2] != p2[2]`.
pub fn densify_bulge(
    p1: [f64; 3],
    p2: [f64; 3],
    bulge: f64,
    max_sagitta: f64,
) -> Result<Vec<[f64; 3]>, GradingError> {
    if !is_finite3(p1) || !is_finite3(p2) || !bulge.is_finite() {
        return Err(GradingError::CalculationOverflow);
    }

    let dx = p2[0] - p1[0];
    let dy = p2[1] - p1[1];
    let chord = dx.hypot(dy);
    if chord < DEGENERATE_TOL {
        return Err(GradingError::DegenerateSegment);
    }
    if bulge.abs() < STRAIGHT_BULGE_TOL {
        return Ok(vec![p1, p2]);
    }

    // Sweep angle and radius from the bulge definition.
    let sweep = 4.0 * bulge.atan();
    let abs_b = bulge.abs();
    let radius = chord * (1.0 + abs_b * abs_b) / (4.0 * abs_b);
    // Distance from chord midpoint to arc center.
    let dist_mid_center = chord * (1.0 - abs_b * abs_b) / (4.0 * abs_b);
    if !radius.is_finite() || !dist_mid_center.is_finite() {
        return Err(GradingError::CalculationOverflow);
    }

    // Left (CCW) unit normal of the chord direction.
    let nx = -dy / chord;
    let ny = dx / chord;
    let mx = (p1[0] + p2[0]) / 2.0;
    let my = (p1[1] + p2[1]) / 2.0;
    // Walking a CCW arc, the center stays left of the chord direction
    // (the arc itself bulges right, away from the center).
    let side = if sweep > 0.0 { 1.0 } else { -1.0 };
    let cx = mx + side * nx * dist_mid_center;
    let cy = my + side * ny * dist_mid_center;
    if !cx.is_finite() || !cy.is_finite() {
        return Err(GradingError::CalculationOverflow);
    }

    let n_segs = arc_segment_count(sweep, radius, max_sagitta);

    let start_angle = (p1[1] - cy).atan2(p1[0] - cx);
    let step = sweep / n_segs as f64;
    let mut pts = Vec::with_capacity(n_segs + 1);
    for i in 0..=n_segs {
        let a = start_angle + step * i as f64;
        let t = i as f64 / n_segs as f64;
        pts.push([
            cx + radius * a.cos(),
            cy + radius * a.sin(),
            p1[2] + t * (p2[2] - p1[2]),
        ]);
    }
    // Pin the endpoints bit-exactly (angle integration drifts ~1e-12).
    pts[0] = p1;
    pts[n_segs] = p2;
    Ok(pts)
}

/// Segment count for an arc, clamped into the range `2..=MAX_SEGMENTS`.
///
/// Derived from the sagitta bound `s = R(1 - cos(phi/2))`, i.e. a maximum
/// half-step of `phi = 2·acos(1 - s/R)`. When that bound collapses — a
/// tolerance finer than `f64` resolution near `acos(1)`, or a non-positive /
/// non-finite `max_sagitta` — the sweep is subdivided either at
/// [`MAX_SEGMENTS`] or at the fixed [`FALLBACK_STEP`] respectively.
fn arc_segment_count(sweep: f64, radius: f64, max_sagitta: f64) -> usize {
    let want = if max_sagitta.is_finite() && max_sagitta > 0.0 && radius.is_finite() && radius > 0.0
    {
        let ratio = (1.0 - max_sagitta / radius).clamp(-1.0, 1.0);
        let max_step = 2.0 * ratio.acos();
        if max_step.is_finite() && max_step > 0.0 {
            sweep.abs() / max_step
        } else {
            // Bound collapsed (division by ~zero): saturate, never `inf as usize`.
            MAX_SEGMENTS as f64
        }
    } else {
        // Unusable tolerance: fixed 22.5° step.
        sweep.abs() / FALLBACK_STEP
    };
    // `f64::{max, min}` ignore NaN, so no input escapes the `2..=MAX_SEGMENTS`
    // window; `inf.min(x)` is `x`, so neither saturation nor OOM is possible.
    want.ceil().max(2.0).min(MAX_SEGMENTS as f64) as usize
}

/// Unit normal of segment `p1 -> p2` on its **right-hand (outward)** side.
///
/// Grading boundaries wind clockwise, so `right = (dy, -dx) / |v|` points away
/// from the enclosed region: eastward segments yield `[0.0, -1.0]`, northward
/// segments `[1.0, 0.0]`.
///
/// # Errors
///
/// * [`GradingError::DegenerateSegment`] — the segment is shorter than
///   `DEGENERATE_TOL`.
/// * [`GradingError::CalculationOverflow`] — any coordinate is non-finite.
pub fn outward_normal_2d(p1: [f64; 2], p2: [f64; 2]) -> Result<[f64; 2], GradingError> {
    if !is_finite2(p1) || !is_finite2(p2) {
        return Err(GradingError::CalculationOverflow);
    }
    let dx = p2[0] - p1[0];
    let dy = p2[1] - p1[1];
    let len = dx.hypot(dy);
    if len < DEGENERATE_TOL {
        return Err(GradingError::DegenerateSegment);
    }
    Ok([dy / len, -dx / len])
}

/// Unit bisector of the two outward normals meeting at vertex `curr` of the
/// polyline `prev -> curr -> next`.
///
/// The sum `N1 + N2` is the direction a corner offset travels: it coincides
/// with the segment normal on a straight run, and bisects the exterior angle
/// at a genuine corner. A 180° turnaround makes the normals cancel, in which
/// case the first leg's normal is returned so the result stays finite and unit
/// length (the bisector of a U-turn is undefined, not NaN).
///
/// # Errors
///
/// Propagates [`outward_normal_2d`]: degenerate legs raise
/// [`GradingError::DegenerateSegment`], non-finite coordinates raise
/// [`GradingError::CalculationOverflow`].
pub fn vertex_bisector_2d(
    prev: [f64; 2],
    curr: [f64; 2],
    next: [f64; 2],
) -> Result<[f64; 2], GradingError> {
    let n1 = outward_normal_2d(prev, curr)?;
    let n2 = outward_normal_2d(curr, next)?;
    let bx = n1[0] + n2[0];
    let by = n1[1] + n2[1];
    let blen = bx.hypot(by);
    if blen < BISECTOR_TOL {
        // Turnaround: outward normals cancel — fall back to the first leg.
        return Ok(n1);
    }
    Ok([bx / blen, by / blen])
}

/// Twice the signed area of triangle `a -> b -> c`.
///
/// Positive when the triple turns counter-clockwise, negative when it turns
/// clockwise, exactly `0.0` for exactly collinear points. This sign convention
/// is what [`is_convex_corner`] reads.
pub fn orient2d(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

/// Whether the corner `prev -> curr -> next` is **convex**: it bulges away
/// from the region enclosed by a clockwise-wound boundary, i.e. it turns
/// right, which makes [`orient2d`] negative.
///
/// Collinear vertices (`orient2d == 0.0`) are not corners and yield `false`,
/// as do non-finite or overflowing results — a corner that cannot be
/// classified is treated as absent rather than guessed.
pub fn is_convex_corner(prev: [f64; 2], curr: [f64; 2], next: [f64; 2]) -> bool {
    let o = orient2d(prev, curr, next);
    o.is_finite() && o < 0.0
}

/// Unit **left** (counter-clockwise) normal of a 2-D segment.
///
/// Returns `None` for a degenerate or non-finite segment. Kept for callers
/// reasoning about the inner side of a boundary; grading itself uses
/// [`outward_normal_2d`].
pub fn left_normal(p1: [f64; 2], p2: [f64; 2]) -> Option<[f64; 2]> {
    outward_normal_2d(p1, p2).ok().map(|n| [-n[0], -n[1]])
}

/// Unit **right** (clockwise, outward) normal of a 2-D segment — the same
/// direction as [`outward_normal_2d`], but `None` instead of an error.
pub fn right_normal(p1: [f64; 2], p2: [f64; 2]) -> Option<[f64; 2]> {
    outward_normal_2d(p1, p2).ok()
}

/// Unit bisector of the interior angle at vertex `curr` of the polyline
/// `prev -> curr -> next`.
///
/// The bisector points along the normalized sum of the incoming and outgoing
/// unit tangents (the miter direction used for corner offsets). Returns `None`
/// when either leg is degenerate or the legs are exactly opposite (a 180°
/// U-turn, where the miter is undefined).
pub fn bisector_at(prev: [f64; 2], curr: [f64; 2], next: [f64; 2]) -> Option<[f64; 2]> {
    let ix = curr[0] - prev[0];
    let iy = curr[1] - prev[1];
    let ox = next[0] - curr[0];
    let oy = next[1] - curr[1];
    if !is_finite2([ix, iy]) || !is_finite2([ox, oy]) {
        return None;
    }
    let ilen = ix.hypot(iy);
    let olen = ox.hypot(oy);
    if ilen < DEGENERATE_TOL || olen < DEGENERATE_TOL {
        return None;
    }
    let bx = ix / ilen + ox / olen;
    let by = iy / ilen + oy / olen;
    let blen = bx.hypot(by);
    if blen < BISECTOR_TOL {
        // Legs exactly oppose: a 180° U-turn, where the miter is undefined.
        // (Straight-through legs instead reinforce to the forward tangent.)
        return None;
    }
    Some([bx / blen, by / blen])
}

#[inline]
fn is_finite2(p: [f64; 2]) -> bool {
    p[0].is_finite() && p[1].is_finite()
}

#[inline]
fn is_finite3(p: [f64; 3]) -> bool {
    is_finite2([p[0], p[1]]) && p[2].is_finite()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::FRAC_1_SQRT_2;
    use std::f64::consts::PI;

    fn max_sagitta_of(pts: &[[f64; 3]], center: [f64; 2], radius: f64) -> f64 {
        pts.iter()
            .map(|p| ((p[0] - center[0]).hypot(p[1] - center[1]) - radius).abs())
            .fold(0.0_f64, f64::max)
    }

    #[test]
    fn straight_bulge_returns_endpoints() {
        let p1 = [0.0, 0.0, 5.0];
        let p2 = [10.0, 0.0, 7.0];
        assert_eq!(densify_bulge(p1, p2, 0.0, 0.01), Ok(vec![p1, p2]));
        assert_eq!(densify_bulge(p1, p2, 1e-12, 0.01), Ok(vec![p1, p2]));
        // Even under the loosest sagitta bound an arc keeps >= 2 segments.
        let arc = densify_bulge(p1, p2, 0.5, 100.0).unwrap();
        assert!(arc.len() >= 3, "arc collapsed to a chord: {}", arc.len());
        assert_eq!(arc[0], p1);
        assert_eq!(arc[arc.len() - 1], p2);
    }

    #[test]
    fn densify_rejects_degenerate_and_non_finite() {
        let p = [3.0, -2.0, 1.5];
        // A zero-length chord is degenerate whatever its bulge is: the check
        // runs before the straight-segment shortcut.
        assert_eq!(
            densify_bulge(p, p, 0.5, 0.01),
            Err(GradingError::DegenerateSegment)
        );
        assert_eq!(
            densify_bulge(p, p, 0.0, 0.01),
            Err(GradingError::DegenerateSegment)
        );
        // Sub-tolerance chords too.
        let q = [p[0] + 1e-12, p[1], p[2]];
        assert_eq!(
            densify_bulge(p, q, 0.5, 0.01),
            Err(GradingError::DegenerateSegment)
        );
        // Non-finite input never leaks NaN points.
        assert_eq!(
            densify_bulge([f64::NAN, 0.0, 0.0], [1.0, 0.0, 0.0], 0.5, 0.01),
            Err(GradingError::CalculationOverflow)
        );
        assert_eq!(
            densify_bulge([0.0, 0.0, 0.0], [f64::NEG_INFINITY, 0.0, 0.0], 0.5, 0.01),
            Err(GradingError::CalculationOverflow)
        );
        assert_eq!(
            densify_bulge([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], f64::INFINITY, 0.01),
            Err(GradingError::CalculationOverflow)
        );
    }

    #[test]
    fn quarter_arc_respects_sagitta_and_elevation() {
        // Chord (1,0)->(0,1) with bulge tan(22.5°): quarter circle R=1 @ origin.
        let bulge = (PI / 8.0).tan();
        let p1 = [1.0, 0.0, 0.0];
        let p2 = [0.0, 1.0, 4.0];
        let pts = densify_bulge(p1, p2, bulge, 0.01).unwrap();
        assert!(pts.len() > 2, "arc must subdivide, got {}", pts.len());
        assert_eq!(pts[0], p1);
        assert_eq!(pts[pts.len() - 1], p2);
        // All points on the circle, mid-arc at 45°.
        for p in &pts {
            let r = p[0].hypot(p[1]);
            assert!((r - 1.0).abs() < 1e-9, "off-circle: {p:?}");
        }
        let mid = pts[pts.len() / 2];
        let expected = FRAC_1_SQRT_2;
        assert!((mid[0] - expected).abs() < 0.05 && (mid[1] - expected).abs() < 0.05);
        // Tightening the tolerance adds segments; sagitta bound holds.
        let fine = densify_bulge(p1, p2, bulge, 0.0005).unwrap();
        assert!(fine.len() > pts.len());
        assert!(max_sagitta_of(&fine, [0.0, 0.0], 1.0) < 0.001);
        // Elevation lerps linearly from 0 to 4 along the arc.
        assert!((pts[0][2] - 0.0).abs() < 1e-12);
        assert!((pts[pts.len() - 1][2] - 4.0).abs() < 1e-12);
        let t = 0.5;
        let n = pts.len() - 1;
        let lo = pts[(t * n as f64).floor() as usize][2];
        let hi = pts[(t * n as f64).ceil() as usize][2];
        assert!((lo - 2.0).abs() < 0.5 && (hi - 2.0).abs() < 0.5);
    }

    #[test]
    fn quarter_arc_elevation_is_monotonic() {
        let bulge = (PI / 8.0).tan();
        let pts = densify_bulge([1.0, 0.0, -2.0], [0.0, 1.0, 6.0], bulge, 0.01).unwrap();
        for w in pts.windows(2) {
            assert!(w[1][2] > w[0][2], "z must strictly increase: {w:?}");
        }
        // Non-uniform elevations are still clamped to the endpoint values.
        assert_eq!(pts[0][2], -2.0);
        assert_eq!(pts[pts.len() - 1][2], 6.0);
    }

    #[test]
    fn semicircle_and_direction_signs() {
        // bulge = tan(45°) = 1: upper half circle R=1 @ origin (CCW).
        let ccw = densify_bulge([1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], 1.0, 0.01).unwrap();
        assert!(ccw.len() > 2);
        assert!(
            ccw.iter().all(|p| p[1] >= -1e-9),
            "CCW must stay above chord"
        );
        assert!((ccw[0][0] - 1.0).abs() < 1e-12);
        for p in &ccw {
            assert!((p[0].hypot(p[1]) - 1.0).abs() < 1e-9);
        }
        // Negative bulge: mirrored lower half (CW).
        let cw = densify_bulge([1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], -1.0, 0.01).unwrap();
        assert!(cw.iter().all(|p| p[1] <= 1e-9), "CW must stay below chord");
        assert_eq!(cw.len(), ccw.len());
        for p in &cw {
            assert!((p[0].hypot(p[1]) - 1.0).abs() < 1e-9);
        }
    }

    #[test]
    fn arc_segment_count_stays_within_bounds() {
        // Tight sagitta bound on a unit quarter circle: at least two segments.
        assert!(arc_segment_count(PI / 4.0, 1.0, 0.01) >= 2);
        // Unusable tolerance -> fixed 22.5° step (exact halvings of PI).
        assert_eq!(arc_segment_count(PI / 4.0, 1.0, 0.0), 2);
        assert_eq!(arc_segment_count(PI, 1.0, -5.0), 8);
        // Ratio finer than f64 resolution saturates at the cap, never usize::MAX.
        assert_eq!(arc_segment_count(4.0 * PI, 2.5e13, 1e-15), MAX_SEGMENTS);
        // A near-zero sweep still yields two segments (>= 3 points).
        assert_eq!(arc_segment_count(1e-12, 1.0, 100.0), 2);
    }

    #[test]
    fn outward_normal_2d_is_unit_right_and_error_free() {
        // Eastward segment: the outward (right) side points south.
        assert_eq!(outward_normal_2d([0.0, 0.0], [4.0, 0.0]), Ok([0.0, -1.0]));
        // Northward segment: outward side points east.
        assert_eq!(outward_normal_2d([0.0, 0.0], [0.0, 4.0]), Ok([1.0, 0.0]));
        // Unit length and orthogonality on a non-axis-aligned segment.
        let n = outward_normal_2d([1.0, 2.0], [4.0, 6.0]).unwrap();
        assert!((n[0].hypot(n[1]) - 1.0).abs() < 1e-12);
        let dir = [3.0, 4.0];
        assert!((n[0] * dir[0] + n[1] * dir[1]).abs() < 1e-12);
        // Degenerate and non-finite inputs are errors, never NaN.
        assert_eq!(
            outward_normal_2d([2.0, 2.0], [2.0, 2.0]),
            Err(GradingError::DegenerateSegment)
        );
        assert_eq!(
            outward_normal_2d([f64::NAN, 0.0], [1.0, 0.0]),
            Err(GradingError::CalculationOverflow)
        );
        assert_eq!(
            outward_normal_2d([0.0, 0.0], [f64::INFINITY, 1.0]),
            Err(GradingError::CalculationOverflow)
        );
    }

    #[test]
    fn vertex_bisector_2d_corner_straight_and_turnaround() {
        // Straight through: both outward normals agree -> the segment normal.
        assert_eq!(
            vertex_bisector_2d([0.0, 0.0], [1.0, 0.0], [2.0, 0.0]),
            Ok([0.0, -1.0])
        );
        // 90° right turn (east then south): normals (0,-1) + (-1,0) -> SW.
        let b = vertex_bisector_2d([0.0, 0.0], [1.0, 0.0], [1.0, -1.0]).unwrap();
        assert!((b[0] + FRAC_1_SQRT_2).abs() < 1e-12);
        assert!((b[1] + FRAC_1_SQRT_2).abs() < 1e-12);
        assert!((b[0].hypot(b[1]) - 1.0).abs() < 1e-12);
        // 180° turnaround: the outward normals cancel -> first leg's normal.
        assert_eq!(
            vertex_bisector_2d([0.0, 0.0], [1.0, 0.0], [0.0, 0.0]),
            Ok([0.0, -1.0])
        );
        // Degenerate legs are errors, never silent NaN.
        assert_eq!(
            vertex_bisector_2d([1.0, 1.0], [1.0, 1.0], [2.0, 1.0]),
            Err(GradingError::DegenerateSegment)
        );
        assert_eq!(
            vertex_bisector_2d([0.0, 0.0], [1.0, 0.0], [f64::NAN, 0.0]),
            Err(GradingError::CalculationOverflow)
        );
    }

    #[test]
    fn is_convex_corner_follows_turn_direction() {
        // Right turn of a clockwise-wound boundary -> convex.
        assert!(is_convex_corner([0.0, 0.0], [1.0, 0.0], [1.0, -1.0]));
        // Left turn -> concave.
        assert!(!is_convex_corner([0.0, 0.0], [1.0, 0.0], [1.0, 1.0]));
        // Collinear vertices are not corners, in either direction.
        assert!(!is_convex_corner([0.0, 0.0], [1.0, 0.0], [2.0, 0.0]));
        assert!(!is_convex_corner([2.0, 0.0], [1.0, 0.0], [0.0, 0.0]));
        // Unclassifiable (non-finite) input is never convex.
        assert!(!is_convex_corner([f64::NAN, 0.0], [1.0, 0.0], [1.0, -1.0]));
        // orient2d sign convention: positive = counter-clockwise.
        assert!(orient2d([0.0, 0.0], [1.0, 0.0], [0.0, 1.0]) > 0.0);
        assert!(orient2d([0.0, 0.0], [0.0, 1.0], [1.0, 0.0]) < 0.0);
        assert_eq!(orient2d([0.0, 0.0], [1.0, 0.0], [2.0, 0.0]), 0.0);
    }

    #[test]
    fn normals_and_bisector() {
        // Eastward segment: left normal is north, right normal is south.
        assert_eq!(left_normal([0.0, 0.0], [4.0, 0.0]), Some([0.0, 1.0]));
        assert_eq!(right_normal([0.0, 0.0], [4.0, 0.0]), Some([0.0, -1.0]));
        assert_eq!(left_normal([1.0, 1.0], [1.0, 1.0]), None);
        assert_eq!(right_normal([1.0, 1.0], [1.0, 1.0]), None);
        // 90° corner (east then north): bisector is NE diagonal.
        let b = bisector_at([0.0, 0.0], [1.0, 0.0], [1.0, 1.0]).unwrap();
        assert!((b[0] - FRAC_1_SQRT_2).abs() < 1e-12 && (b[1] - FRAC_1_SQRT_2).abs() < 1e-12);
        // Degenerate legs and U-turns are None, never NaN.
        assert_eq!(bisector_at([0.0, 0.0], [0.0, 0.0], [1.0, 0.0]), None);
        assert_eq!(bisector_at([0.0, 0.0], [1.0, 0.0], [0.0, 0.0]), None);
        assert_eq!(bisector_at([f64::NAN, 0.0], [1.0, 0.0], [2.0, 0.0]), None);
        // Straight-through: tangents reinforce to the forward direction.
        assert_eq!(
            bisector_at([0.0, 0.0], [1.0, 0.0], [2.0, 0.0]),
            Some([1.0, 0.0])
        );
    }
}
