//! Breakline integration for TIN surfaces.
//!
//! This module provides constrained Delaunay triangulation for integrating
//! breaklines into TIN surfaces, matching Civil 3D breakline behavior.

use crate::featureline::{BreaklineResult, BreaklineType, FeatureLine};
use crate::surface::Surface;
use std::collections::{HashMap, HashSet};

/// Constrained edge for triangulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ConstrainedEdge {
    a: usize,
    b: usize,
}

impl ConstrainedEdge {
    fn new(a: usize, b: usize) -> Self {
        if a < b {
            Self { a, b }
        } else {
            Self { a: b, b: a }
        }
    }
}

/// Add a feature line as a breakline to a surface using constrained Delaunay triangulation.
/// This modifies the surface's triangulation to enforce the breakline edges.
pub fn add_breakline(
    surface: &mut Surface,
    breakline: &FeatureLine,
    _breakline_type: BreaklineType,
) -> Result<BreaklineResult, String> {
    if breakline.vertices.len() < 2 {
        return Err("Breakline must have at least 2 vertices".into());
    }

    let mut result = BreaklineResult::default();
    let mut constrained_edges = HashSet::new();

    // Step 1: Insert breakline vertices into surface triangulation
    let vertex_indices = insert_breakline_vertices(surface, breakline, &mut result)?;

    // Step 2: Create constrained edges from breakline segments
    for i in 0..vertex_indices.len() - 1 {
        constrained_edges.insert(ConstrainedEdge::new(
            vertex_indices[i],
            vertex_indices[i + 1],
        ));
    }

    // Step 3: Recover constrained edges by edge flipping
    recover_constrained_edges(surface, &constrained_edges)?;

    result.triangles_modified = surface
        .triangles
        .len()
        .saturating_sub(result.triangles_before);
    result.triangles_added = surface
        .triangles
        .len()
        .saturating_sub(result.triangles_before);
    result.affected_area = calculate_affected_area(surface, &vertex_indices);

    Ok(result)
}

/// Insert breakline vertices into surface triangulation, returning their node indices.
fn insert_breakline_vertices(
    surface: &mut Surface,
    breakline: &FeatureLine,
    result: &mut BreaklineResult,
) -> Result<Vec<usize>, String> {
    let mut indices = Vec::with_capacity(breakline.vertices.len());
    let mut coord_to_index = HashMap::new();

    // Build lookup of existing nodes (with tolerance)
    for (idx, node) in surface.nodes.iter().enumerate() {
        let key = (round_coord(node[0]), round_coord(node[1]));
        coord_to_index.insert(key, idx);
    }

    // Store initial triangle count
    result.triangles_before = surface.triangles.len();

    for vertex in &breakline.vertices {
        let key = (round_coord(vertex.pt.x), round_coord(vertex.pt.y));

        if let Some(&existing_idx) = coord_to_index.get(&key) {
            // Vertex already exists - update Z if breakline is standard type
            indices.push(existing_idx);
        } else {
            // Insert new vertex into triangulation
            let idx =
                insert_vertex_into_triangulation(surface, vertex.pt.x, vertex.pt.y, vertex.pt.z)?;
            coord_to_index.insert((round_coord(vertex.pt.x), round_coord(vertex.pt.y)), idx);
            indices.push(idx);
            result.vertices_added += 1;
        }
    }

    Ok(indices)
}

/// Insert a vertex into the triangulation by finding the containing triangle and splitting it.
fn insert_vertex_into_triangulation(
    surface: &mut Surface,
    x: f64,
    y: f64,
    z: f64,
) -> Result<usize, String> {
    // Add the new node
    surface.nodes.push([x, y, z]);

    // Find the containing triangle
    let (tri_idx, bary) = match surface.find_containing_triangle(x, y) {
        Some((idx, (wa, wb, wc))) => (idx, (wa, wb, wc)),
        None => {
            // Point is outside the convex hull - add to nodes but don't triangulate yet
            // It will be handled during re-triangulation
            return Ok(surface.nodes.len() - 1);
        }
    };

    // Check if point is on an edge (barycentric coordinate is zero)
    let eps = 1e-9;
    let (wa, wb, wc) = bary;

    if wa < eps || wb < eps || wc < eps {
        // Point is on an edge or vertex - handle edge split case
        // For simplicity, we'll mark for re-triangulation and let the constraint recovery handle it
        // In a full implementation, we'd do a 2-to-4 split here
        return Ok(surface.nodes.len() - 1);
    }

    // Point is inside triangle - split triangle into 3
    let tri = surface.triangles[tri_idx];
    let new_idx = surface.nodes.len() - 1;

    // Remove the original triangle
    surface.triangles.remove(tri_idx);

    // Add 3 new triangles: (a, b, new), (b, c, new), (c, a, new)
    surface.triangles.push([tri[0], tri[1], new_idx]);
    surface.triangles.push([tri[1], tri[2], new_idx]);
    surface.triangles.push([tri[2], tri[0], new_idx]);

    Ok(new_idx)
}

/// Round coordinate for lookup tolerance (1mm precision).
fn round_coord(c: f64) -> i64 {
    (c * 1000.0).round() as i64
}

/// Recover constrained edges by edge flipping - ensures breakline edges exist in triangulation.
fn recover_constrained_edges(
    surface: &mut Surface,
    constraints: &HashSet<ConstrainedEdge>,
) -> Result<(), String> {
    if constraints.is_empty() {
        return Ok(());
    }

    // Build adjacency for fast edge lookup
    let mut edge_to_triangles: HashMap<ConstrainedEdge, Vec<usize>> = HashMap::new();
    for (tri_idx, tri) in surface.triangles.iter().enumerate() {
        let edges = [
            ConstrainedEdge::new(tri[0], tri[1]),
            ConstrainedEdge::new(tri[1], tri[2]),
            ConstrainedEdge::new(tri[2], tri[0]),
        ];
        for edge in edges {
            edge_to_triangles.entry(edge).or_default().push(tri_idx);
        }
    }

    // Mark constrained edges
    let mut is_constrained = HashSet::new();
    for edge in constraints {
        is_constrained.insert(*edge);
    }

    // Incremental edge flipping to satisfy constraints
    let max_iterations = surface.triangles.len() * 10;
    let mut iterations = 0;

    loop {
        let mut flipped = false;
        let mut flip_ops = Vec::new();

        // Find a triangle edge that violates Delaunay but is not constrained
        for (tri_idx, tri) in surface.triangles.iter().enumerate() {
            let edges = [
                (ConstrainedEdge::new(tri[0], tri[1]), (tri[0], tri[1])),
                (ConstrainedEdge::new(tri[1], tri[2]), (tri[1], tri[2])),
                (ConstrainedEdge::new(tri[2], tri[0]), (tri[2], tri[0])),
            ];

            for (edge, (a, b)) in edges {
                if is_constrained.contains(&edge) {
                    continue; // Don't flip constrained edges
                }

                // Check if edge is shared by two triangles
                if let Some(adj_tris) = edge_to_triangles.get(&edge) {
                    if adj_tris.len() == 2 {
                        let other_tri_idx = if adj_tris[0] == tri_idx {
                            adj_tris[1]
                        } else {
                            adj_tris[0]
                        };
                        let other_tri = surface.triangles[other_tri_idx];

                        // Opposite corner of the *neighbouring* triangle, resolved
                        // against the shared edge (a, b) actually under
                        // consideration. Matching against `tri[0]`/`tri[1]` is
                        // only correct for the first of the three edges, and for
                        // the other two it can yield an edge endpoint.
                        if let Some(d) = opposite_vertex(other_tri, a, b) {
                            // Check Delaunay condition: is the opposite vertex inside the circumcircle?
                            if !is_delaunay(surface, tri[0], tri[1], tri[2], d) {
                                // Queue the flip operation
                                flip_ops.push((tri_idx, other_tri_idx, a, b));
                            }
                        }
                    }
                }
            }
            if !flip_ops.is_empty() {
                break;
            }
        }

        // Apply all collected flip operations
        for (tri_idx, other_tri_idx, a, b) in flip_ops {
            flip_edge(surface, tri_idx, other_tri_idx, a, b)?;
            flipped = true;
            break;
        }

        if !flipped {
            break; // No more flips needed
        }

        iterations += 1;
        if iterations > max_iterations {
            break; // Safety limit
        }
    }

    // Now ensure all constrained edges exist in the triangulation
    // For each constrained edge, if it doesn't exist, we need to enforce it
    for constrained_edge in constraints {
        ensure_edge_exists(
            surface,
            *constrained_edge,
            &mut edge_to_triangles,
            &is_constrained,
        )?;
    }

    Ok(())
}

/// Ensure a constrained edge exists in the triangulation by edge flipping.
fn ensure_edge_exists(
    surface: &mut Surface,
    constrained_edge: ConstrainedEdge,
    edge_to_triangles: &mut HashMap<ConstrainedEdge, Vec<usize>>,
    is_constrained: &HashSet<ConstrainedEdge>,
) -> Result<(), String> {
    // Check if edge already exists
    if edge_to_triangles.contains_key(&constrained_edge) {
        return Ok(());
    }

    // Find a path between the two vertices and flip edges to create the constrained edge
    // This is a simplified implementation - a full implementation would use a path-finding algorithm
    // For now, we'll use a simple approach: find a path and flip edges along it

    // Build adjacency graph
    let mut adj: HashMap<usize, Vec<usize>> = HashMap::new();
    for (edge, _) in edge_to_triangles.iter() {
        adj.entry(edge.a).or_default().push(edge.b);
        adj.entry(edge.b).or_default().push(edge.a);
    }

    // BFS to find path from a to b
    let start = constrained_edge.a;
    let goal = constrained_edge.b;
    let mut queue = std::collections::VecDeque::new();
    let mut visited = HashSet::new();
    let mut parent: HashMap<usize, usize> = HashMap::new();

    queue.push_back(start);
    visited.insert(start);

    let mut found = false;
    while let Some(current) = queue.pop_front() {
        if current == goal {
            found = true;
            break;
        }
        if let Some(neighbors) = adj.get(&current) {
            for &neighbor in neighbors {
                if !visited.contains(&neighbor) {
                    visited.insert(neighbor);
                    parent.insert(neighbor, current);
                    queue.push_back(neighbor);
                }
            }
        }
    }

    if !found {
        return Err("Could not find path between constrained edge vertices".into());
    }

    // Reconstruct path and flip edges along it
    let mut path = Vec::new();
    let mut current = goal;
    while current != start {
        path.push(current);
        current = parent[&current];
    }
    path.push(start);
    path.reverse();

    // Flip edges along the path to create the constrained edge
    for i in 0..path.len() - 1 {
        let a = path[i];
        let b = path[i + 1];
        let edge = ConstrainedEdge::new(a, b);

        // If this edge is not constrained, flip it
        if !is_constrained.contains(&edge) {
            // Find the two triangles sharing this edge
            if let Some(adj_tris) = edge_to_triangles.get(&edge) {
                if adj_tris.len() == 2 {
                    let tri1_idx = adj_tris[0];
                    let tri2_idx = adj_tris[1];

                    // Both opposite corners are derived inside `flip_edge` from
                    // the shared edge `(edge.a, edge.b)`. The edge can occupy any
                    // of a triangle's three corner pairs, so deriving them here
                    // against `tri1[0..1]` would hand `flip_edge` an edge
                    // endpoint as if it were the opposite corner and yield a
                    // degenerate replacement triangle.
                    flip_edge(surface, tri1_idx, tri2_idx, edge.a, edge.b)?;
                }
            }
        }
    }

    Ok(())
}

/// Check if point d is outside the circumcircle of triangle abc (Delaunay condition).
/// Returns true if Delaunay condition is satisfied (d is outside or on the circle).
fn is_delaunay(surface: &Surface, a: usize, b: usize, c: usize, d: usize) -> bool {
    let pa = surface.nodes[a];
    let pb = surface.nodes[b];
    let pc = surface.nodes[c];
    let pd = surface.nodes[d];

    // Use 2D coordinates for circumcircle test (Z doesn't matter for planarity)
    let ax = pa[0];
    let ay = pa[1];
    let bx = pb[0];
    let by = pb[1];
    let cx = pc[0];
    let cy = pc[1];
    let dx = pd[0];
    let dy = pd[1];

    // Translate so d is at origin
    let adx = ax - dx;
    let ady = ay - dy;
    let bdx = bx - dx;
    let bdy = by - dy;
    let cdx = cx - dx;
    let cdy = cy - dy;

    // Compute determinant
    let det = adx * (bdy * (cdx * cdx + cdy * cdy) - cdy * (bdx * bdx + bdy * bdy))
        - ady * (bdx * (cdx * cdx + cdy * cdy) - cdx * (bdx * bdx + bdy * bdy))
        + (adx * adx + ady * ady) * (bdx * cdy - bdy * cdx);

    // For CCW triangle, det > 0 means D is inside circumcircle (not Delaunay)
    // We need to check triangle orientation
    let orient = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax);

    if orient > 0.0 {
        det <= 1e-12 // CCW: Delaunay if det <= 0
    } else {
        det >= -1e-12 // CW: Delaunay if det >= 0
    }
}

/// Vertex of `tri` opposite the shared edge `(a, b)`.
///
/// Returns `None` when `tri` is degenerate (a repeated index) or when `(a, b)`
/// is not a corner pair of `tri`, which would make an edge flip unsound.
fn opposite_vertex(tri: [usize; 3], a: usize, b: usize) -> Option<usize> {
    if a == b || tri[0] == tri[1] || tri[1] == tri[2] || tri[0] == tri[2] {
        return None;
    }
    let mut found: Option<usize> = None;
    for &v in tri.iter() {
        if v == a || v == b {
            continue;
        }
        if found.is_some() {
            // Two vertices differ from (a, b): the edge is not a corner pair.
            return None;
        }
        found = Some(v);
    }
    found
}

/// Flip an edge shared by two triangles.
///
/// The quadrilateral is `a, b` (the shared edge) with opposite corners `c` in
/// `tri1_idx` and `d` in `tri2_idx`. Both corners are derived from the triangles
/// themselves: a shared edge can sit at any of a triangle's three corner pairs,
/// so a caller-supplied corner is not necessarily the vertex opposite the edge.
fn flip_edge(
    surface: &mut Surface,
    tri1_idx: usize,
    tri2_idx: usize,
    a: usize,
    b: usize,
) -> Result<(), String> {
    if a == b {
        return Err("edge flip requires two distinct edge endpoints".into());
    }

    let tri1 = surface.triangles[tri1_idx];
    let tri2 = surface.triangles[tri2_idx];

    let c = opposite_vertex(tri1, a, b)
        .ok_or_else(|| format!("edge ({a}, {b}) is not a corner pair of triangle {tri1_idx}"))?;
    let d = opposite_vertex(tri2, a, b)
        .ok_or_else(|| format!("edge ({a}, {b}) is not a corner pair of triangle {tri2_idx}"))?;

    if c == d {
        return Err("edge flip requires two triangles with distinct opposite corners".into());
    }

    // The edge being flipped is between vertices a and b; the new triangles
    // are (a, c, d) and (b, d, c) with proper orientation.
    let pa = surface.nodes[a];
    let pc = surface.nodes[c];
    let pd = surface.nodes[d];

    // Create new triangles with proper orientation
    let (new_tri1, new_tri2) = if orient2d(pa, pc, pd) > 0.0 {
        ([a, c, d], [b, d, c])
    } else {
        ([a, d, c], [b, c, d])
    };

    // Replace triangles
    surface.triangles[tri1_idx] = new_tri1;
    surface.triangles[tri2_idx] = new_tri2;

    Ok(())
}

/// 2D orientation test: positive if a->b->c is CCW.
pub fn orient2d(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

/// Calculate affected area around breakline vertices.
fn calculate_affected_area(surface: &Surface, vertex_indices: &[usize]) -> f64 {
    // Build vertex-to-triangle mapping on the fly
    let mut vertex_to_tris: std::collections::HashMap<usize, Vec<usize>> =
        std::collections::HashMap::new();
    for (tri_idx, tri) in surface.triangles.iter().enumerate() {
        for &v_idx in tri {
            vertex_to_tris.entry(v_idx).or_default().push(tri_idx);
        }
    }

    let mut area = 0.0;
    for &idx in vertex_indices {
        if let Some(tris) = vertex_to_tris.get(&idx) {
            for &tri_idx in tris {
                if tri_idx < surface.triangles.len() {
                    area += triangle_area(surface, surface.triangles[tri_idx]);
                }
            }
        }
    }
    area
}

fn triangle_area(surface: &Surface, tri: [usize; 3]) -> f64 {
    let a = surface.nodes[tri[0]];
    let b = surface.nodes[tri[1]];
    let c = surface.nodes[tri[2]];

    let ax = a[0];
    let ay = a[1];
    let bx = b[0];
    let by = b[1];
    let cx = c[0];
    let cy = c[1];

    ((bx - ax) * (cy - ay) - (by - ay) * (cx - ax)).abs() * 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unit square with the shared diagonal-edge pair, stored so that the shared
    /// edge (1, 2) is *not* `tri1[0..1]` — the arrangement that made the previous
    /// `tri1[0..1]`-based corner lookup pick an edge endpoint as the opposite
    /// corner and emit a degenerate triangle.
    fn square_surface() -> Surface {
        let mut surface = Surface::default();
        surface.name = "flip-regression".to_string();
        surface.nodes = vec![
            [0.0, 0.0, 0.0], // 0
            [1.0, 0.0, 0.0], // 1
            [1.0, 1.0, 0.0], // 2
            [0.0, 1.0, 0.0], // 3
        ];
        // Shared edge is (1, 2): at positions (0, 2) in tri1 and (0, 1) in tri2.
        surface.triangles = vec![[1, 0, 2], [2, 1, 3]];
        surface
    }

    fn distinct(tri: [usize; 3]) -> bool {
        tri[0] != tri[1] && tri[1] != tri[2] && tri[0] != tri[2]
    }

    #[test]
    fn opposite_vertex_resolves_corners_for_every_corner_pair() {
        // The edge is the right-hand side of the square: (1, 2).
        assert_eq!(opposite_vertex([1, 0, 2], 1, 2), Some(0));
        assert_eq!(opposite_vertex([2, 1, 3], 1, 2), Some(3));
        // Order of the endpoints must not matter.
        assert_eq!(opposite_vertex([1, 0, 2], 2, 1), Some(0));
    }

    #[test]
    fn opposite_vertex_rejects_non_corner_pair_and_degenerate_input() {
        // Only one of (0, 3) is in the triangle: two vertices differ -> not a corner pair.
        assert_eq!(opposite_vertex([1, 0, 2], 0, 3), None);
        // Repeated index leaves two differing vertices.
        assert_eq!(opposite_vertex([1, 1, 2], 1, 2), None);
        // Repeated index on an edge endpoint: only one vertex differs, so the
        // count check alone would wrongly accept it.
        assert_eq!(opposite_vertex([1, 1, 3], 1, 2), None);
        assert_eq!(opposite_vertex([1, 3, 3], 1, 2), None);
        assert_eq!(opposite_vertex([3, 1, 1], 1, 2), None);
        // Identical endpoints cannot describe a shared edge.
        assert_eq!(opposite_vertex([1, 0, 2], 2, 2), None);
        // Fully disjoint.
        assert_eq!(opposite_vertex([1, 0, 2], 5, 6), None);
    }

    #[test]
    fn flip_edge_derives_opposite_corners_from_shared_edge() {
        let mut surface = square_surface();

        // Edge (1, 2) is deliberately not stored at tri1[0..1].
        flip_edge(&mut surface, 0, 1, 1, 2).expect("flip should succeed");

        let t1 = surface.triangles[0];
        let t2 = surface.triangles[1];

        assert!(distinct(t1), "triangle 1 became degenerate: {:?}", t1);
        assert!(distinct(t2), "triangle 2 became degenerate: {:?}", t2);

        // The new triangles must span the same four vertices, none of which is
        // shared between the two new triangles except the new diagonal.
        let mut all: Vec<usize> = t1.iter().chain(t2.iter()).copied().collect();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all, vec![0, 1, 2, 3], "vertex set changed during flip");

        // The opposite corners (0, 3) become the new shared edge, and the old
        // shared edge (1, 2) is gone.
        let edges = [
            ConstrainedEdge::new(t1[0], t1[1]),
            ConstrainedEdge::new(t1[1], t1[2]),
            ConstrainedEdge::new(t2[0], t2[1]),
            ConstrainedEdge::new(t2[1], t2[2]),
        ];
        assert!(edges.contains(&ConstrainedEdge::new(0, 3)));
        for e in edges {
            assert_ne!(e, ConstrainedEdge::new(1, 2));
        }
    }

    #[test]
    fn flip_edge_result_preserves_total_area() {
        let mut surface = square_surface();
        let before: f64 = surface
            .triangles
            .iter()
            .map(|&t| triangle_area(&surface, t))
            .sum();

        flip_edge(&mut surface, 0, 1, 1, 2).expect("flip should succeed");

        let after: f64 = surface
            .triangles
            .iter()
            .map(|&t| triangle_area(&surface, t))
            .sum();

        assert!(
            (before - after).abs() < 1e-12,
            "flip changed area: {before} -> {after}"
        );
    }

    #[test]
    fn flip_edge_rejects_invalid_input_without_mutating() {
        let mut surface = square_surface();
        let snapshot = surface.triangles.clone();

        // (0, 3) is not an edge of tri1 = [1, 0, 2].
        assert!(flip_edge(&mut surface, 0, 1, 0, 3).is_err());
        assert_eq!(surface.triangles, snapshot);

        // Identical endpoints cannot describe a shared edge.
        assert!(flip_edge(&mut surface, 0, 1, 1, 1).is_err());
        assert_eq!(surface.triangles, snapshot);
    }
}
