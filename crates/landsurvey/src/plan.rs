//! Plan geometry definitions for landsurvey engine.
//!
//! This module defines the data structures for parsed plan JSON and provides
//! methods to extract geometric information for CAD entity generation.
//!
//! The plan JSON format supports:
//! - Lines: [x1, y1, x2, y2, layer]
//! - Arcs: [cx, cy, r, start_deg, end_deg, layer]
//! - Circles: [cx, cy, r, layer]
//! - Texts: [x, y, value, layer]
//! - Polylines: [[x, y[, bulge][, z]], ..., layer] where each point can have
//!   2 (x,y), 3 (x,y,bulge), or 4 (x,y,bulge,z) numbers, and the final
//!   element is the layer string.

use serde_json::{Error as JsonError, Value};
use std::collections::HashSet;
use std::io;

/// A segment key representing a line segment between two points.
/// The key is normalized so that the segment (x1,y1,x2,y2) and (x2,y2,x1,y1)
/// produce the same key, enabling deduplication regardless of direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SegKey {
    pub x1: i64,
    pub y1: i64,
    pub x2: i64,
    pub y2: i64,
}

impl Eq for SegKey {}

impl std::hash::Hash for SegKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.x1.hash(state);
        self.y1.hash(state);
        self.x2.hash(state);
        self.y2.hash(state);
    }
}

/// Create a normalized segment key from two points.
/// The points are ordered lexicographically (by x, then y) to ensure
/// the same key for either direction.
/// Coordinates are scaled by 1e6 and rounded to integers for stable hashing.
pub fn seg_key(x1: f64, y1: f64, x2: f64, y2: f64) -> SegKey {
    const SCALE: f64 = 1_000_000.0;
    let ix1 = (x1 * SCALE).round() as i64;
    let iy1 = (y1 * SCALE).round() as i64;
    let ix2 = (x2 * SCALE).round() as i64;
    let iy2 = (y2 * SCALE).round() as i64;
    // Order the points lexicographically to make the key direction-independent.
    if (ix1 < ix2) || (ix1 == ix2 && iy1 <= iy2) {
        SegKey {
            x1: ix1,
            y1: iy1,
            x2: ix2,
            y2: iy2,
        }
    } else {
        SegKey {
            x1: ix2,
            y1: iy2,
            x2: ix1,
            y2: iy1,
        }
    }
}

/// A single point in a polyline, with optional bulge and Z elevation.
pub type PolylinePoint = (f64, f64, f64, Option<f64>);

/// A polyline entity from a plan.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanPolyline {
    /// The vertices of the polyline. Each vertex is (x, y, bulge, z).
    /// Bulge represents the tangent of the arc from this vertex to the next.
    /// Z elevation is optional; None indicates no elevation (2D).
    pub points: Vec<PolylinePoint>,
    /// The layer on which this polyline resides.
    pub layer: String,
}

impl PlanPolyline {
    /// Returns the vertices and a flag indicating whether the polyline is closed.
    ///
    /// If the first and last points have the same (x, y) coordinates (within
    /// a small tolerance), the polyline is considered closed. In that case,
    /// the returned vertices exclude the duplicate final point, and the
    /// closed flag is true. Otherwise, all vertices are returned and closed
    /// is false.
    pub fn ring(&self) -> (Vec<PolylinePoint>, bool) {
        if self.points.len() < 2 {
            return (self.points.clone(), false);
        }
        let first = &self.points[0];
        let last = &self.points[self.points.len() - 1];
        // Compare only x and y for closure; ignore bulge and Z.
        let epsilon = 1e-9;
        let same_xy = (first.0 - last.0).abs() < epsilon && (first.1 - last.1).abs() < epsilon;
        if same_xy && self.points.len() > 2 {
            // Exclude the last point if it duplicates the first.
            let vertices = self.points[0..self.points.len() - 1].to_vec();
            (vertices, true)
        } else {
            (self.points.clone(), false)
        }
    }

    /// Returns the set of segment keys for all edges in the polyline.
    ///
    /// For a closed polyline, edges are between consecutive vertices and
    /// from the last vertex to the first. For an open polyline, edges are
    /// between consecutive vertices only.
    pub fn edge_keys(&self) -> HashSet<SegKey> {
        let mut keys = HashSet::new();
        let (pts, closed) = self.ring();
        let n = pts.len();
        if n == 0 {
            return keys;
        }
        for i in 0..n {
            let j = if closed { (i + 1) % n } else { i + 1 };
            if j >= n {
                break;
            }
            let p1 = &pts[i];
            let p2 = &pts[j];
            let key = seg_key(p1.0, p1.1, p2.0, p2.1);
            keys.insert(key);
        }
        keys
    }

    /// Returns the set of segment keys for edges that have a non-zero bulge.
    ///
    /// For each vertex, if its bulge is non-zero, the edge from that vertex
    /// to the next vertex (considering closure) is considered an arc segment.
    /// Returns tuples of (x1, y1, x2, y2, bulge).
    pub fn bulged_segments(&self) -> Vec<(f64, f64, f64, f64, f64)> {
        let mut segs = Vec::new();
        let (pts, closed) = self.ring();
        let n = pts.len();
        if n == 0 {
            return segs;
        }
        for i in 0..n {
            let bulge = pts[i].2;
            if bulge.abs() > 1e-12 {
                let j = if closed { (i + 1) % n } else { i + 1 };
                if j >= n {
                    break;
                }
                let p1 = &pts[i];
                let p2 = &pts[j];
                segs.push((p1.0, p1.1, p2.0, p2.1, bulge));
            }
        }
        segs
    }
}

/// A line segment from a plan.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanLine {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub layer: String,
}

/// A circular arc from a plan.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanArc {
    pub cx: f64,
    pub cy: f64,
    pub radius: f64,
    pub start_deg: f64,
    pub end_deg: f64,
    pub layer: String,
}

/// A circle from a plan.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanCircle {
    pub cx: f64,
    pub cy: f64,
    pub radius: f64,
    pub layer: String,
}

/// A text entity from a plan.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanText {
    pub x: f64,
    pub y: f64,
    pub value: String,
    pub layer: String,
}

/// The top-level plan structure.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub polylines: Vec<PlanPolyline>,
    pub lines: Vec<PlanLine>,
    pub arcs: Vec<PlanArc>,
    pub circles: Vec<PlanCircle>,
    pub texts: Vec<PlanText>,
}

fn err_custom(msg: &str) -> JsonError {
    JsonError::io(io::Error::new(io::ErrorKind::InvalidData, msg))
}

fn err_invalid_type() -> JsonError {
    JsonError::io(io::Error::new(io::ErrorKind::InvalidData, "invalid type"))
}

impl Plan {
    /// Parse a plan JSON string into a Plan structure.
    ///
    /// The JSON is expected to have the following optional fields:
    /// - `polylines`: array of polyline specifications
    /// - `lines`: array of line specifications
    /// - `arcs`: array of arc specifications
    /// - `circles`: array of circle specifications
    /// - `texts`: array of text specifications
    ///
    /// See the module documentation for the format of each array element.
    pub fn parse(json: &str) -> Result<Self, JsonError> {
        let value: Value = serde_json::from_str(json)?;
        let empty_vec: Vec<Value> = vec![];
        let get_array = |key: &str| {
            value
                .get(key)
                .and_then(|v| v.as_array())
                .unwrap_or(&empty_vec)
        };

        let mut polylines = Vec::new();
        let mut lines = Vec::new();
        let mut arcs = Vec::new();
        let mut circles = Vec::new();
        let mut texts = Vec::new();

        // Parse polylines
        for pval in get_array("polylines") {
            if let Some(arr) = pval.as_array() {
                if arr.is_empty() {
                    continue;
                }
                // The last element should be the layer string.
                let layer_val = arr.last().unwrap();
                let layer = layer_val
                    .as_str()
                    .ok_or(err_custom("Layer must be a string"))?
                    .to_string();
                // All preceding elements are points.
                let mut points = Vec::new();
                for pt_val in &arr[0..arr.len() - 1] {
                    if let Some(pt_arr) = pt_val.as_array() {
                        let (x, y, bulge, z) = match pt_arr.len() {
                            2 => {
                                let x = pt_arr[0].as_f64().ok_or(err_invalid_type())?;
                                let y = pt_arr[1].as_f64().ok_or(err_invalid_type())?;
                                (x, y, 0.0, None)
                            }
                            3 => {
                                let x = pt_arr[0].as_f64().ok_or(err_invalid_type())?;
                                let y = pt_arr[1].as_f64().ok_or(err_invalid_type())?;
                                let bulge = pt_arr[2].as_f64().ok_or(err_invalid_type())?;
                                (x, y, bulge, None)
                            }
                            4 => {
                                let x = pt_arr[0].as_f64().ok_or(err_invalid_type())?;
                                let y = pt_arr[1].as_f64().ok_or(err_invalid_type())?;
                                let bulge = pt_arr[2].as_f64().ok_or(err_invalid_type())?;
                                let z = pt_arr[3].as_f64().ok_or(err_invalid_type())?;
                                (x, y, bulge, Some(z))
                            }
                            _ => {
                                return Err(err_custom(&format!(
                                    "Invalid point length {}, expected 2, 3, or 4",
                                    pt_arr.len()
                                )));
                            }
                        };
                        points.push((x, y, bulge, z));
                    } else {
                        return Err(err_custom("Point must be an array"));
                    }
                }
                polylines.push(PlanPolyline { points, layer });
            } else {
                return Err(err_custom("Polyline must be an array"));
            }
        }

        // Parse lines
        for lval in get_array("lines") {
            if let Some(arr) = lval.as_array() {
                if arr.len() != 5 {
                    return Err(err_custom("Line must have 5 elements"));
                }
                let x1 = arr[0].as_f64().ok_or(err_invalid_type())?;
                let y1 = arr[1].as_f64().ok_or(err_invalid_type())?;
                let x2 = arr[2].as_f64().ok_or(err_invalid_type())?;
                let y2 = arr[3].as_f64().ok_or(err_invalid_type())?;
                let layer = arr[4]
                    .as_str()
                    .ok_or(err_custom("Layer must be a string"))?
                    .to_string();
                lines.push(PlanLine {
                    x1,
                    y1,
                    x2,
                    y2,
                    layer,
                });
            } else {
                return Err(err_custom("Line must be an array"));
            }
        }

        // Parse arcs
        for aval in get_array("arcs") {
            if let Some(arr) = aval.as_array() {
                if arr.len() != 6 {
                    return Err(err_custom("Arc must have 6 elements"));
                }
                let cx = arr[0].as_f64().ok_or(err_invalid_type())?;
                let cy = arr[1].as_f64().ok_or(err_invalid_type())?;
                let radius = arr[2].as_f64().ok_or(err_invalid_type())?;
                let start_deg = arr[3].as_f64().ok_or(err_invalid_type())?;
                let end_deg = arr[4].as_f64().ok_or(err_invalid_type())?;
                let layer = arr[5]
                    .as_str()
                    .ok_or(err_custom("Layer must be a string"))?
                    .to_string();
                arcs.push(PlanArc {
                    cx,
                    cy,
                    radius,
                    start_deg,
                    end_deg,
                    layer,
                });
            } else {
                return Err(err_custom("Arc must be an array"));
            }
        }

        // Parse circles
        for cval in get_array("circles") {
            if let Some(arr) = cval.as_array() {
                if arr.len() != 4 {
                    return Err(err_custom("Circle must have 4 elements"));
                }
                let cx = arr[0].as_f64().ok_or(err_invalid_type())?;
                let cy = arr[1].as_f64().ok_or(err_invalid_type())?;
                let radius = arr[2].as_f64().ok_or(err_invalid_type())?;
                let layer = arr[3]
                    .as_str()
                    .ok_or(err_custom("Layer must be a string"))?
                    .to_string();
                circles.push(PlanCircle {
                    cx,
                    cy,
                    radius,
                    layer,
                });
            } else {
                return Err(err_custom("Circle must be an array"));
            }
        }

        // Parse texts
        for tval in get_array("texts") {
            if let Some(arr) = tval.as_array() {
                if arr.len() != 4 {
                    return Err(err_custom("Text must have 4 elements"));
                }
                let x = arr[0].as_f64().ok_or(err_invalid_type())?;
                let y = arr[1].as_f64().ok_or(err_invalid_type())?;
                let value = arr[2]
                    .as_str()
                    .ok_or(err_custom("Value must be a string"))?
                    .to_string();
                let layer = arr[3]
                    .as_str()
                    .ok_or(err_custom("Layer must be a string"))?
                    .to_string();
                texts.push(PlanText { x, y, value, layer });
            } else {
                return Err(err_custom("Text must be an array"));
            }
        }

        Ok(Plan {
            polylines,
            lines,
            arcs,
            circles,
            texts,
        })
    }
}

/// Parse a plan JSON string into a Plan structure.
///
/// The JSON is expected to have the following optional fields:
/// - `polylines`: array of polyline specifications
/// - `lines`: array of line specifications
/// - `arcs`: array of arc specifications
/// - `circles`: array of circle specifications
/// - `texts`: array of text specifications
///
/// See the module documentation for the format of each array element.
pub fn parse(json: &str) -> Result<Plan, JsonError> {
    let value: Value = serde_json::from_str(json)?;
    let empty_vec: Vec<Value> = vec![];
    let get_array = |key: &str| {
        value
            .get(key)
            .and_then(|v| v.as_array())
            .unwrap_or(&empty_vec)
    };

    let mut polylines = Vec::new();
    let mut lines = Vec::new();
    let mut arcs = Vec::new();
    let mut circles = Vec::new();
    let mut texts = Vec::new();

    // Parse polylines
    for pval in get_array("polylines") {
        if let Some(arr) = pval.as_array() {
            if arr.is_empty() {
                continue;
            }
            // The last element should be the layer string.
            let layer_val = arr.last().unwrap();
            let layer = layer_val
                .as_str()
                .ok_or(err_custom("Layer must be a string"))?
                .to_string();
            // All preceding elements are points.
            let mut points = Vec::new();
            for pt_val in &arr[0..arr.len() - 1] {
                if let Some(pt_arr) = pt_val.as_array() {
                    let (x, y, bulge, z) = match pt_arr.len() {
                        2 => {
                            let x = pt_arr[0].as_f64().ok_or(err_invalid_type())?;
                            let y = pt_arr[1].as_f64().ok_or(err_invalid_type())?;
                            (x, y, 0.0, None)
                        }
                        3 => {
                            let x = pt_arr[0].as_f64().ok_or(err_invalid_type())?;
                            let y = pt_arr[1].as_f64().ok_or(err_invalid_type())?;
                            let bulge = pt_arr[2].as_f64().ok_or(err_invalid_type())?;
                            (x, y, bulge, None)
                        }
                        4 => {
                            let x = pt_arr[0].as_f64().ok_or(err_invalid_type())?;
                            let y = pt_arr[1].as_f64().ok_or(err_invalid_type())?;
                            let bulge = pt_arr[2].as_f64().ok_or(err_invalid_type())?;
                            let z = pt_arr[3].as_f64().ok_or(err_invalid_type())?;
                            (x, y, bulge, Some(z))
                        }
                        _ => {
                            return Err(err_custom(&format!(
                                "Invalid point length {}, expected 2, 3, or 4",
                                pt_arr.len()
                            )));
                        }
                    };
                    points.push((x, y, bulge, z));
                } else {
                    return Err(err_custom("Point must be an array"));
                }
            }
            polylines.push(PlanPolyline { points, layer });
        } else {
            return Err(err_custom("Polyline must be an array"));
        }
    }

    // Parse lines
    for lval in get_array("lines") {
        if let Some(arr) = lval.as_array() {
            if arr.len() != 5 {
                return Err(err_custom("Line must have 5 elements"));
            }
            let x1 = arr[0].as_f64().ok_or(err_invalid_type())?;
            let y1 = arr[1].as_f64().ok_or(err_invalid_type())?;
            let x2 = arr[2].as_f64().ok_or(err_invalid_type())?;
            let y2 = arr[3].as_f64().ok_or(err_invalid_type())?;
            let layer = arr[4]
                .as_str()
                .ok_or(err_custom("Layer must be a string"))?
                .to_string();
            lines.push(PlanLine {
                x1,
                y1,
                x2,
                y2,
                layer,
            });
        } else {
            return Err(err_custom("Line must be an array"));
        }
    }

    // Parse arcs
    for aval in get_array("arcs") {
        if let Some(arr) = aval.as_array() {
            if arr.len() != 6 {
                return Err(err_custom("Arc must have 6 elements"));
            }
            let cx = arr[0].as_f64().ok_or(err_invalid_type())?;
            let cy = arr[1].as_f64().ok_or(err_invalid_type())?;
            let radius = arr[2].as_f64().ok_or(err_invalid_type())?;
            let start_deg = arr[3].as_f64().ok_or(err_invalid_type())?;
            let end_deg = arr[4].as_f64().ok_or(err_invalid_type())?;
            let layer = arr[5]
                .as_str()
                .ok_or(err_custom("Layer must be a string"))?
                .to_string();
            arcs.push(PlanArc {
                cx,
                cy,
                radius,
                start_deg,
                end_deg,
                layer,
            });
        } else {
            return Err(err_custom("Arc must be an array"));
        }
    }

    // Parse circles
    for cval in get_array("circles") {
        if let Some(arr) = cval.as_array() {
            if arr.len() != 4 {
                return Err(err_custom("Circle must have 4 elements"));
            }
            let cx = arr[0].as_f64().ok_or(err_invalid_type())?;
            let cy = arr[1].as_f64().ok_or(err_invalid_type())?;
            let radius = arr[2].as_f64().ok_or(err_invalid_type())?;
            let layer = arr[3]
                .as_str()
                .ok_or(err_custom("Layer must be a string"))?
                .to_string();
            circles.push(PlanCircle {
                cx,
                cy,
                radius,
                layer,
            });
        } else {
            return Err(err_custom("Circle must be an array"));
        }
    }

    // Parse texts
    for tval in get_array("texts") {
        if let Some(arr) = tval.as_array() {
            if arr.len() != 4 {
                return Err(err_custom("Text must have 4 elements"));
            }
            let x = arr[0].as_f64().ok_or(err_invalid_type())?;
            let y = arr[1].as_f64().ok_or(err_invalid_type())?;
            let value = arr[2]
                .as_str()
                .ok_or(err_custom("Value must be a string"))?
                .to_string();
            let layer = arr[3]
                .as_str()
                .ok_or(err_custom("Layer must be a string"))?
                .to_string();
            texts.push(PlanText { x, y, value, layer });
        } else {
            return Err(err_custom("Text must be an array"));
        }
    }

    Ok(Plan {
        polylines,
        lines,
        arcs,
        circles,
        texts,
    })
}

/// Decode text from the plan format (handles special characters like %%d for degrees).
pub fn decode_text(s: &str) -> String {
    s.replace("%%d", "°")
}

/// Check if a flattened arc entry duplicates a bulged chain segment.
/// Returns true if the arc parameters match the segment within tolerance.
/// Handles segments in both forward and reverse directions.
pub fn arc_duplicates_segment(
    cx: f64,
    cy: f64,
    r: f64,
    start_deg: f64,
    end_deg: f64,
    seg: (f64, f64, f64, f64, f64),
    tol: f64,
) -> bool {
    let (sx1, sy1, sx2, sy2, sbulge) = seg;
    // Quick bounding-box rejection
    if (cx - sx1).abs() > r + tol
        || (cy - sy1).abs() > r + tol
        || (cx - sx2).abs() > r + tol
        || (cy - sy2).abs() > r + tol
    {
        return false;
    }
    // More precise check: compute the arc's chord and compare to segment
    let start_rad = start_deg.to_radians();
    let end_rad = end_deg.to_radians();
    let ax1 = cx + r * start_rad.sin();
    let ay1 = cy + r * start_rad.cos();
    let ax2 = cx + r * end_rad.sin();
    let ay2 = cy + r * end_rad.cos();
    // Check forward direction match: arc start→end matches segment start→end
    let chord_match_fwd = (ax1 - sx1).abs() < tol
        && (ay1 - sy1).abs() < tol
        && (ax2 - sx2).abs() < tol
        && (ay2 - sy2).abs() < tol;
    // Check reverse direction match: arc start→end matches segment end→start
    let chord_match_rev = (ax1 - sx2).abs() < tol
        && (ay1 - sy2).abs() < tol
        && (ax2 - sx1).abs() < tol
        && (ay2 - sy1).abs() < tol;
    if !chord_match_fwd && !chord_match_rev {
        return false;
    }
    // Compare bulge: for a circular arc, bulge = tan((end - start)/4)
    let sweep = end_rad - start_rad;
    let expected_bulge = (sweep / 4.0).tan();
    (sbulge - expected_bulge).abs() < tol
}
