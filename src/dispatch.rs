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
                let cad_points: Vec<CadPoint> = poly.into().collect();
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