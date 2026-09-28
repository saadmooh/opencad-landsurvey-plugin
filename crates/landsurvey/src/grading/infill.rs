//! Grading surface and infill generation for the land-survey kernel.
//!
//! Layer-C module (`std` only — no host/CAD types).

use crate::featureline::entity::{FeatureLine, FeatureVertex};
use crate::surface::Surface;
use crate::grading::{
    daylight::{generate_daylight_line},
    criteria::GradingCriteria,
    error::GradingError,
};

/// Mesh the side slope quad strips between the baseline featureline and the daylight line.
///
/// Builds a triangulated irregular network (TIN) bridging consecutive stations on the baseline
/// with their corresponding projected stations and radial fan rays on the daylight line.
///
/// # Arguments
///
/// * `baseline` - The original feature line to grade
/// * `daylight` - The daylight line generated from projecting the baseline
///
/// # Returns
///
/// A Surface representing the side slopes, or GradingError on failure
pub fn mesh_grading_slopes(
    baseline: &FeatureLine,
    daylight: &FeatureLine,
) -> Result<Surface, GradingError> {
    // Ensure both lines have the same number of points for 1:1 correspondence
    if baseline.vertex_count() != daylight.vertex_count() {
        return Err(GradingError::InvalidCriteria(
            "Baseline and daylight lines must have matching vertex counts for slope meshing"
                .to_string(),
        ));
    }

    // Handle degenerate cases
    if baseline.vertex_count() < 2 {
        return Ok(Surface::default());
    }

    let mut nodes = Vec::new();
    let mut triangles = Vec::new();

    // Add all baseline and daylight points to the nodes vector
    // Baseline points: indices 0..baseline_len-1
    // Daylight points: indices baseline_len..baseline_len+daylight_len-1
    let baseline_len = baseline.vertex_count();

    for vertex in baseline.vertices.iter() {
        nodes.push([vertex.pt.x, vertex.pt.y, vertex.pt.z]);
    }

    for vertex in daylight.vertices.iter() {
        nodes.push([vertex.pt.x, vertex.pt.y, vertex.pt.z]);
    }

    // Create triangles connecting baseline[i] -> baseline[i+1] -> daylight[i+1] -> daylight[i]
    // This creates a quad strip that we split into two triangles
    for i in 0..baseline_len - 1 {
        // Triangle 1: baseline[i], baseline[i+1], daylight[i+1]
        triangles.push([i, i + 1, baseline_len + i + 1]);
        // Triangle 2: baseline[i], daylight[i+1], daylight[i]
        triangles.push([i, baseline_len + i + 1, baseline_len + i]);
    }

    Ok(Surface {
        name: String::new(),
        nodes,
        triangles,
        outer_boundary: None,
        hide_boundaries: Vec::new(),
    })
}

/// Mesh the grading pad infill for closed featurelines.
///
/// If the baseline featureline forms a closed loop, collects all baseline 3D vertices
/// and triangulates using constrained Delaunay logic.
///
/// # Arguments
///
/// * `baseline` - The feature line to check for closure and potentially infill
///
/// # Returns
///
/// A Surface representing the infill pad, or GradingError on failure
pub fn mesh_grading_infill(baseline: &FeatureLine) -> Result<Surface, GradingError> {
    // Check if the baseline is closed (first point == last point within tolerance)
    if baseline.vertex_count() < 3 {
        return Ok(Surface::default()); // Not enough points to form a closed area
    }

    let first_point = &baseline.vertices[0].pt;
    let last_point = &baseline.vertices[baseline.vertex_count() - 1].pt;

    let is_closed = (first_point.x - last_point.x).abs() < 1e-9
        && (first_point.y - last_point.y).abs() < 1e-9
        && (first_point.z - last_point.z).abs() < 1e-9;

    if !is_closed {
        // Not closed, return empty surface (no infill)
        return Ok(Surface::default());
    }

    // Collect all 3D vertices (excluding the duplicate last point if closed)
    let mut nodes = Vec::new();
    let vertex_count = if is_closed {
        baseline.vertex_count() - 1
    } else {
        baseline.vertex_count()
    };

    for i in 0..vertex_count {
        let vertex = &baseline.vertices[i].pt;
        nodes.push([vertex.x, vertex.y, vertex.z]);
    }

    // Create surface from points (Delaunay triangulation)
    let mut surface = Surface::from_points(&nodes);

    // Apply outer boundary using the 2D polygon of the baseline
    let mut boundary_polygon = Vec::new();
    for i in 0..vertex_count {
        let vertex = &baseline.vertices[i].pt;
        boundary_polygon.push([vertex.x, vertex.y]);
    }

    // Apply the outer boundary constraint
    match surface.apply_outer_boundary(&boundary_polygon) {
        Ok(_) => {}
        Err(e) => {
            return Err(GradingError::InvalidCriteria(format!(
                "Failed to apply outer boundary for grading infill: {}",
                e
            )));
        }
    }

    Ok(surface)
}

/// Create a complete grading surface combining infill pad and side slopes.
///
/// This is the main entry point for grading surface generation:
/// 1. Generate the daylight line from the baseline featureline
/// 2. If the baseline is closed, create an infill pad surface
/// 3. Create the side slope surface between baseline and daylight
/// 4. Combine infill and slopes into a single coherent Surface
///
/// # Arguments
///
/// * `baseline` - The feature line to grade
/// * `criteria` - Grading criteria defining slopes and targets
/// * `target_surface` - The existing ground surface to grade against
///
/// # Returns
///
/// A Surface representing the complete grading design, or GradingError on failure
pub fn create_grading_surface(
    baseline: &FeatureLine,
    criteria: &GradingCriteria,
    target_surface: &Surface,
) -> Result<Surface, GradingError> {
    // Step 1: Generate the daylight line
    let daylight_line = generate_daylight_line(baseline, criteria, target_surface)?;

    // Step 2: Create infill pad surface if baseline is closed
    let mut combined_surface = Surface::default();

    let infill_surface = mesh_grading_infill(baseline)?;
    if !infill_surface.nodes.is_empty() && !infill_surface.triangles.is_empty() {
        // Combine infill surface with our working surface
        if combined_surface.nodes.is_empty() {
            combined_surface = infill_surface;
        } else {
            // Merge two surfaces (simple concatenation for now)
            let mut merged_nodes = combined_surface.nodes.clone();
            merged_nodes.extend(infill_surface.nodes.into_iter());
            let mut merged_triangles = combined_surface.triangles.clone();
            // Offset triangle indices by the size of the first surface's node array
            let offset = combined_surface.nodes.len();
            for tri in infill_surface.triangles {
                merged_triangles.push([tri[0] + offset, tri[1] + offset, tri[2] + offset]);
            }
            combined_surface = Surface {
                name: String::new(),
                nodes: merged_nodes,
                triangles: merged_triangles,
                outer_boundary: None,
                hide_boundaries: Vec::new(),
            };
        }
    }

    // Step 3: Create side slope surface
    let slope_surface = mesh_grading_slopes(baseline, &daylight_line)?;

    // Step 4: Combine slope surface with our working surface
    if combined_surface.nodes.is_empty() {
        combined_surface = slope_surface;
    } else if !slope_surface.nodes.is_empty() && !slope_surface.triangles.is_empty() {
        // Merge two surfaces
        let mut merged_nodes = combined_surface.nodes.clone();
        merged_nodes.extend(slope_surface.nodes.into_iter());
        let mut merged_triangles = combined_surface.triangles.clone();
        // Offset triangle indices by the size of the first surface's node array
        let offset = combined_surface.nodes.len();
        for tri in slope_surface.triangles {
            merged_triangles.push([tri[0] + offset, tri[1] + offset, tri[2] + offset]);
        }
        combined_surface = Surface {
            name: String::new(),
            nodes: merged_nodes,
            triangles: merged_triangles,
            outer_boundary: None,
            hide_boundaries: Vec::new(),
        };
    }

    Ok(combined_surface)
}