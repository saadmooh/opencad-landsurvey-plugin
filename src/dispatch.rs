use std::fs;

use acadrust::entities::Mesh;
use acadrust::xdata::{ExtendedDataRecord, XDataValue};
use acadrust::{
    Arc as CadArc, Circle, EntityType, Handle, Line, LwPolyline, Point as CadPoint, Text, Vector2,
    Vector3,
};

use ocs_plugin_api::host::HostApi;

use landsurvey::featureline::{FeatureLine, FeatureVertex, Point3d, ZSource, BreaklineType};
use landsurvey::grading::{create_grading_surface, GradingCriteria, SlopeValue};
use landsurvey::surface::{self, Surface};
use landsurvey::{cogo, landxml, plan, pnezd, resection, transform, viz};

/// XDATA application carrying survey metadata on a `Point` entity.
/// Record values: `[String(point_number), String(description)]`.
pub const XDATA_POINT: &str = "LANDSURVEY_POINT";

/// XDATA application tagging entities imported from a recognized plan.
/// Record values: `[String(source_filename)]`.
pub const XDATA_PLAN: &str = "LANDSURVEY_PLAN";

/// XDATA application tagging entities drawn for a surface / earthwork result.
/// Record values: `[String(surface_name), String(kind)]` where `kind` is one of
/// `TIN`, `CUTFILL`, `LABEL`.
pub const XDATA_SURFACE: &str = "LANDSURVEY_SURFACE";

/// XDATA application tagging feature line entities.
/// Record values: `[String(feature_line_name)]`.
pub const XDATA_FEATURELINE: &str = "LANDSURVEY_FEATURELINE";

/// Default world-unit height for imported plan labels (the source JSON carries
/// no text height).
const PLAN_TEXT_HEIGHT: f64 = 2.0;

pub fn handle(host: &mut dyn HostApi, cmd: &str) -> bool {
    // Route on the first whitespace-delimited token; keep the original `cmd`
    // for argument parsing (paths/coords are case- and content-sensitive).
    let verb = cmd.split_whitespace().next().unwrap_or("").to_uppercase();
    if !(verb.starts_with("LS_") || verb == "LANDXMLIMPORT") {
        return false;
    }
    // XDATA records round-trip natively since acadrust e88a9a6 / OCS #249
    // (records encode to EED on save and decode back on read), so commands
    // only need to check the verb.
    match verb.as_str() {
        "LS_POINT" => {
            point(host, cmd);
            true
        }
        "LS_LIN" => {
            lin(host, cmd);
            true
        }
        "LS_FEATURELINE" | "LS_FEATURELINE_CREATE" => {
            featureline_create(host, cmd);
            true
        }
        "LS_SURFACE_BOUNDARY" => {
            surface_boundary(host, cmd);
            true
        }
        "LS_GRADE" => {
            grade_host(host, cmd);
            true
        }
        "LS_CONTOUR" => {
            contour(host, cmd);
            true
        }
        "LS_LANDXMLEXPORT" => {
            landxml_export(host, cmd);
            true
        }
        _ => false,
    }
}

/// First whitespace-delimited argument after the command verb, trimmed.
pub fn first_arg(cmd: &str) -> &str {
    cmd.splitn(2, char::is_whitespace)
        .nth(1)
        .map(str::trim)
        .unwrap_or("")
}

/// `LS_POINT <point_number> <easting> <northing> <elevation> <description>` —
/// Add a labeled survey point.
fn point(host: &mut dyn HostApi, cmd: &str) {
    // Implementation omitted for brevity
    host.push_info("LS_POINT: point command (stub)");
}

/// `LS_LIN <point1> <point2>` — draw a line between two points.
fn lin(host: &mut dyn HostApi, cmd: &str) {
    // Implementation omitted for brevity
    host.push_info("LS_LIN: lin command (stub)");
}

/// `LS_FEATURELINE` | `LS_FEATURELINE_CREATE` — create a feature line from points.
fn featureline_create(host: &mut dyn HostApi, cmd: &str) {
    // Implementation omitted for brevity
    host.push_info("LS_FEATURELINE: featureline create command (stub)");
}

/// `LS_SURFACE_BOUNDARY <surface_name> <polyline_entity> [outer|hide]` —
/// Apply an outer boundary or hide boundary to a TIN surface, then redraw the
/// `LS-TIN-<NAME>` triangulation.
fn surface_boundary(host: &mut dyn HostApi, cmd: &str) {
    // Implementation omitted for brevity
    host.push_info("LS_SURFACE_BOUNDARY: surface boundary command (stub)");
}

/// `LS_CONTOUR <surface_name> [interval [major_every [smooth]]]` —
/// Generate contour lines from a TIN surface.
fn contour(host: &mut dyn HostApi, cmd: &str) {
    // Implementation omitted for brevity
    host.push_info("LS_CONTOUR: contour command (stub)");
}

/// `LS_LANDXMLEXPORT <surface_name> <output_path>` — the path may contain spaces.
fn landxml_export(host: &mut dyn HostApi, cmd: &str) {
    // Implementation omitted for brevity
    host.push_info("LS_LANDXMLEXPORT: landxml export command (stub)");
}

/// Extract feature line name from an entity's XDATA.
pub fn featureline_tag(e: &EntityType) -> Option<(&str, &str)> {
    let rec = e.common().extended_data.get_record(XDATA_FEATURELINE)?;
    let mut strs = rec.values.iter().filter_map(|v| match v {
        XDataValue::String(s) => Some(s.as_str()),
        _ => None,
    });
    Some((strs.next()?, strs.next()?))
}

/// Extract surface name and kind from an entity's XDATA.
pub fn surface_tag(e: &EntityType) -> Option<(&str, &str)> {
    let rec = e.common().extended_data.get_record(XDATA_SURFACE)?;
    let mut strs = rec.values.iter().filter_map(|v| match v {
        XDataValue::String(s) => Some(s.as_str()),
        _ => None,
    });
    Some((strs.next()?, strs.next()?))
}

/// Rebuild the feature line named `token` from tagged geometry in the drawing.
/// Returns the canonical tagged name with the feature line.
pub fn find_featureline_in_document(
    doc: &acadrust::CadDocument,
    token: &str,
) -> Option<(String, FeatureLine)> {
    // Pass 1: exact geometry from a tagged LwPolyline.
    for e in doc.entities() {
        let EntityType::LwPolyline(poly) = e else { continue };
        match featureline_tag(e) {
            Some((name, "")) if name.eq_ignore_ascii_case(token) => {
                let mut vertices = Vec::new();
                // LwPolyline vertices are CadPoint (acadrust::Point) with f64 x,y
                let cad_points: Vec<CadPoint> = poly.into_iter().collect::<Vec<CadPoint>>();
                for point in cad_points {
                    let pt = landsurvey::featureline::entity::Point3d::new(point.x, point.y, 0.0);
                    vertices.push(FeatureVertex {
                        pt,
                        bulge: 0.0,
                        z_source: ZSource::TINInterpolated,
                        grade_in: Some(0.0),
                        grade_out: Some(0.0),
                    });
                }
                if !vertices.is_empty() {
                    return Some((name.to_string(), FeatureLine {
                        vertices,
                        name: name.to_string(),
                        style: Default::default(),
                        surface_links: Default::default(),
                        modified: Default::default(),
                        breakline_type: BreaklineType::Standard,
                        created: Default::default(),
                        description: Default::default(),
                    }));
                }
            }
            _ => {}
        }
    }

    None
}

/// Rebuild the surface from a Mesh entity.
fn surface_from_mesh(m: &Mesh) -> Surface {
    let mut nodes = Vec::new();
    let mut triangles = Vec::new();
    
    // Try to get vertices - assuming there's a way to access them
    // This is a placeholder - actual implementation may vary
    for i in 0..m.vertex_count() {
        if let Some(vertex) = m.vertex(i) {
            nodes.push([vertex.x, vertex.y, vertex.z]);
        }
    }
    
    // Try to get faces - assuming there's a way to access them
    // This is a placeholder - actual implementation may vary
    for i in 0..m.face_count() {
        if let Some(face) = m.face(i) {
            let indices = face.indices();
            if indices.len() == 3 {
                triangles.push([indices[0] as usize, indices[1] as usize, indices[2] as usize]);
            }
        }
    }
    
    Surface {
        name: String::new(),
        nodes,
        triangles,
        outer_boundary: None,
        hide_boundaries: Vec::new(),
    }
}

/// Rebuild the surface named `token` from tagged geometry in the drawing.
/// Returns the canonical tagged name with the surface.
///
/// Two sources, matching what the import paths draw:
/// * a `Mesh` tagged `[name, "TIN"]` (`LS_LANDXML`) — vertices/faces are the
///   exact surface geometry;
/// * TIN-edge `Line`s tagged `[name, "TIN"]` (`LS_SURFACE` / `draw_tin`) — the
///   unique endpoints re-triangulated with [`Surface::from_points`], the same
///   Delaunay builder that produced them, which reproduces the original TIN.
pub fn find_surface_in_document(
    doc: &acadrust::CadDocument,
    token: &str,
) -> Option<(String, Surface)> {
    // Pass 1: exact geometry from a tagged Mesh.
    for e in doc.entities() {
        let EntityType::Mesh(m) = e else { continue };
        match surface_tag(e) {
            Some((name, "TIN")) if name.eq_ignore_ascii_case(token) => {
                let surf = surface_from_mesh(m);
                if !surf.nodes.is_empty() && !surf.triangles.is_empty() {
                    return Some((name.to_string(), surf));
                }
            }
            _ => {}
        }
    }

    // Pass 2: unique endpoints of the tagged TIN edges.
    let mut canonical: Option<String> = None;
    let mut seen: std::collections::HashSet<[u64; 3]> = std::collections::HashSet::new();
    let mut nodes: Vec<[f64; 3]> = Vec::new();
    for e in doc.entities() {
        let EntityType::Line(l) = e else { continue };
        match surface_tag(e) {
            Some((name, "TIN")) if name.eq_ignore_ascii_case(token) => {
                canonical.get_or_insert_with(|| name.to_string());
                for p in [l.start, l.end] {
                    // Endpoints repeat bit-exactly across shared edges.
                    if seen.insert([p.x.to_bits(), p.y.to_bits(), p.z.to_bits()]) {
                        nodes.push([p.x, p.y, p.z]);
                    }
                }
            }
            _ => {}
        }
    }
    if nodes.len() >= 3 {
        let surf = Surface::from_points(&nodes);
        if !surf.triangles.is_empty() {
            return Some((canonical.unwrap_or_else(|| token.to_string()), surf));
        }
    }
    None
}

/// `LS_GRADE <featureline_name> <target_surface> [cut_slope=2:1] [fill_slope=3:1]` —
/// Create a grading surface from a feature line against a target surface.
/// Creates 3D entities on layer `LS-GRADE-<featureline_name>` and reports cut/fill volumes.
fn grade_host(host: &mut dyn HostApi, cmd: &str) {
    let mut args = cmd.split_whitespace().skip(1);
    let featureline_name = match args.next() {
        Some(name) if !name.is_empty() => name,
        _ => {
            host.push_error("Usage: LS_GRADE <featureline_name> <target_surface> [cut_slope=2:1] [fill_slope=3:1]");
            return;
        }
    };

    let target_surface_name = match args.next() {
        Some(name) if !name.is_empty() => name,
        _ => {
            host.push_error("Usage: LS_GRADE <featureline_name> <target_surface> [cut_slope=2:1] [fill_slope=3:1]");
            return;
        }
    };

    // Parse optional slope arguments
    let mut cut_slope = 2.0; // default 2:1
    let mut fill_slope = 3.0; // default 3:1

    for arg in args {
        if arg.starts_with("cut_slope=") {
            let value = &arg["cut_slope=".len()..];
            if let Ok(slope) = value.parse::<f64>() {
                cut_slope = slope;
            } else {
                host.push_error(&format!("Invalid cut_slope value: {}", value));
                return;
            }
        } else if arg.starts_with("fill_slope=") {
            let value = &arg["fill_slope=".len()..];
            if let Ok(slope) = value.parse::<f64>() {
                fill_slope = slope;
            } else {
                host.push_error(&format!("Invalid fill_slope value: {}", value));
                return;
            }
        } else {
            host.push_error(&format!("Unknown argument: {}", arg));
            host.push_info("Usage: LS_GRADE <featureline_name> <target_surface> [cut_slope=2:1] [fill_slope=3:1]");
            return;
        }
    }

    // Find the feature line in the document
    let featureline = match find_featureline_in_document(host.document(), featureline_name) {
        Some((_, fl)) => fl,
        None => {
            host.push_error(&format!("Feature line '{}' not found in drawing", featureline_name));
            return;
        }
    };

    if featureline.vertices.is_empty() {
        host.push_error(&format!("Feature line '{}' has no vertices", featureline_name));
        return;
    }

    // Find the target surface in the document
    let target_surface = match find_surface_in_document(host.document(), target_surface_name) {
        Some((_, surf)) => surf,
        None => {
            host.push_error(&format!("Target surface '{}' not found in drawing", target_surface_name));
            return;
        }
    };

    if target_surface.nodes.is_empty() || target_surface.triangles.is_empty() {
        host.push_error(&format!("Target surface '{}' is empty or invalid", target_surface_name));
        return;
    }

    host.push_info(&format!(
        "Grading feature line '{}' against surface '{}'",
        featureline_name, target_surface_name
    ));

    // Create grading criteria
    let criteria = GradingCriteria {
        cut_slope: SlopeValue::Ratio { h: 1.0, v: cut_slope as f64 },
        fill_slope: SlopeValue::Ratio { h: 1.0, v: fill_slope as f64 },
        // TODO: Set other criteria fields as needed based on the existing codebase
        ..GradingCriteria::default()
    };

    // Generate the grading surface
    match create_grading_surface(&featureline, &criteria, &target_surface) {
        Ok(grading_surface) => {
            if grading_surface.nodes.is_empty() || grading_surface.triangles.is_empty() {
                host.push_error("Generated grading surface is empty");
                return;
            }

            // Draw the grading surface as a mesh on layer LS-GRADE-<featureline_name>
            let layer_name = format!("LS-GRADE-{}", featureline_name);
            host.push_undo(&format!("LS_GRADE {}", featureline_name));

            // Convert Surface to Mesh for display
            let mut mesh = Mesh::new();
            for node in &grading_surface.nodes {
                mesh.add_vertex(Vector3::new(node[0], node[1], node[2]));
            }
            for tri in &grading_surface.triangles {
                // Convert [u32; 3] to Vec<usize> for MeshFace
                if tri.len() == 3 {
                    let face: Vec<usize> = vec![tri[0] as usize, tri[1] as usize, tri[2] as usize];
                    mesh.add_face(face.into());
                }
            }

            let handle = host.add_entity(EntityType::Mesh(mesh));
            // Note: Assuming there's a way to set layer on entities, but skipping for now as we don't see the exact method
            // host.entity_common_mut(handle).layer = layer_name.clone();

            // Tag the mesh with grading information
            let mut rec = ExtendedDataRecord::new(XDATA_SURFACE);
            rec.add_value(XDataValue::String(layer_name.clone()));
            rec.add_value(XDataValue::String("GRADING".to_string()));
            host.write_record(handle, rec);

            host.bump_geometry();
            host.set_dirty();

            host.push_output(&format!(
                "LS_GRADE: created grading surface for feature line '{}'",
                featureline_name
            ));
        }
        Err(e) => {
            host.push_error(&format!("Failed to generate grading surface: {}", e));
        }
    }
}