//! TIN surfaces + earthwork volumes + contours + boundaries — pure functions,
//! `std` only (no host/CAD/iced/acadrust), so this builds unchanged for CLI/WASM.
//!
//! A [`Surface`] is `nodes` (`[E, N, Z]`) + `triangles` (index triples into
//! `nodes`). Volumes use exact per-facet integration: for a planar TIN triangle
//! the prism volume to a datum is `area_xy * mean_z` (exact, since the facet is
//! linear). Three earthwork paths are provided:
//!
//! * [`Surface::cut_fill_to_datum`] — surface vs a horizontal plane (exact).
//! * [`grid_cut_fill`] — two independent surfaces by the grid (column) method
//!   (approximate, step-sensitive; matches the way MicroSurvey's Earthwork
//!   tutorial samples a TOP/BOTTOM pair that do not share triangulation).
//! * [`exact_composite_cut_fill`] — two independent surfaces by exact TIN
//!   overlay (clip each top facet against each bottom facet; on every convex
//!   overlay cell both surfaces are linear so `dz = top - bottom` is linear and
//!   integrates exactly, split at the `dz = 0` contour for cut/fill).
//!
//! Convention matches [`crate::cogo`]: Easting -> world X, Northing -> world Y.

use serde::{Deserialize, Serialize};

/// A node carries `[easting (x), northing (y), elevation (z)]`.
pub type Node = [f64; 3];
/// A triangle is three indices into a surface's `nodes`.
pub type Tri = [usize; 3];

const EPS: f64 = 1e-12;

/// Cut / fill / net earthwork volumes (cubic world units).
///
/// `cut` is material above the reference (top above bottom, or surface above
/// datum); `fill` is below; `net = cut - fill`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CutFill {
    pub cut: f64,
    pub fill: f64,
    pub net: f64,
}

impl CutFill {
    fn from_cut_fill(cut: f64, fill: f64) -> Self {
        CutFill {
            cut,
            fill,
            net: cut - fill,
        }
    }
}

/// Result of the grid (column) volume between two independent surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GridVolume {
    pub cut: f64,
    pub fill: f64,
    pub net: f64,
    /// Plan area of cells where both surfaces were defined.
    pub plan_area: f64,
    /// Number of contributing cells.
    pub n_cells: usize,
}

/// Detailed result of the exact TIN-overlay volume between two surfaces.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompositeDetail {
    pub cut_fill: CutFill,
    /// Cut/fill boundary line: `dz = 0` crossing segments (plan view).
    pub cutfill_line: Vec<[[f64; 2]; 2]>,
}

/// Contour line at a specific elevation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContourLine {
    pub elevation: f64,
    pub is_major: bool,
    pub points: Vec<[f64; 3]>,
}

/// A triangulated irregular network with optional boundary clipping.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Surface {
    /// The surface name.
    pub name: String,
    /// The nodes (Easting, Northing, Elevation).
    pub nodes: Vec<Node>,
    /// The triangles (indices into `nodes`).
    pub triangles: Vec<Tri>,
    /// Outer boundary polygon — triangles outside are clipped.
    #[serde(default)]
    pub outer_boundary: Option<Vec<[f64; 2]>>,
    /// Hide boundaries (interior holes) — triangles inside are removed.
    #[serde(default)]
    pub hide_boundaries: Vec<Vec<[f64; 2]>>,
}

impl Default for Surface {
    fn default() -> Self {
        Surface {
            name: String::new(),
            nodes: Vec::new(),
            triangles: Vec::new(),
            outer_boundary: None,
            hide_boundaries: Vec::new(),
        }
    }
}

impl Surface {
    /// Build a TIN by Delaunay-triangulating the points' XY.
    pub fn from_points(points: &[Node]) -> Surface {
        let xy: Vec<[f64; 2]> = points.iter().map(|p| [p[0], p[1]]).collect();
        Surface {
            name: String::new(),
            nodes: points.to_vec(),
            triangles: delaunay(&xy),
            outer_boundary: None,
            hide_boundaries: Vec::new(),
        }
    }

    /// Create a named surface from points.
    pub fn from_points_named(name: String, points: &[Node]) -> Surface {
        let mut s = Self::from_points(points);
        s.name = name;
        s
    }

    fn tri_area_xy(&self, t: Tri) -> f64 {
        let (a, b, c) = (self.nodes[t[0]], self.nodes[t[1]], self.nodes[t[2]]);
        ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])).abs() / 2.0
    }

    /// Total 2-D (plan) area of the triangulation.
    pub fn area_2d(&self) -> f64 {
        self.triangles.iter().map(|&t| self.tri_area_xy(t)).sum()
    }

    /// Unique TIN edges as node-index pairs (each shared edge listed once).
    /// Used to draw the triangulation without doubling shared edges.
    pub fn edges(&self) -> Vec<[usize; 2]> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for &t in &self.triangles {
            for e in [[t[0], t[1]], [t[1], t[2]], [t[2], t[0]]] {
                let key = if e[0] < e[1] {
                    (e[0], e[1])
                } else {
                    (e[1], e[0])
                };
                if seen.insert(key) {
                    out.push([key.0, key.1]);
                }
            }
        }
        out
    }

    /// Plan-view bounding box `(min_x, max_x, min_y, max_y)` of the nodes.
    pub fn extent(&self) -> (f64, f64, f64, f64) {
        let mut minx = f64::INFINITY;
        let mut maxx = f64::NEG_INFINITY;
        let mut miny = f64::INFINITY;
        let mut maxy = f64::NEG_INFINITY;
        for n in &self.nodes {
            minx = minx.min(n[0]);
            maxx = maxx.max(n[0]);
            miny = miny.min(n[1]);
            maxy = maxy.max(n[1]);
        }
        (minx, maxx, miny, maxy)
    }

    /// Signed net volume between the surface and a horizontal `datum` plane
    /// (above datum positive). Exact for planar TIN facets.
    pub fn volume_to_datum(&self, datum: f64) -> f64 {
        self.triangles
            .iter()
            .map(|&t| {
                let zmean = (self.nodes[t[0]][2] + self.nodes[t[1]][2] + self.nodes[t[2]][2]) / 3.0;
                self.tri_area_xy(t) * (zmean - datum)
            })
            .sum()
    }

    /// Exact cut/fill/net volumes vs a horizontal `datum` plane. Triangles that
    /// cross the datum are split exactly at the datum contour.
    pub fn cut_fill_to_datum(&self, datum: f64) -> CutFill {
        self.cut_fill_to_datum_detailed(datum).0
    }

    /// As [`Surface::cut_fill_to_datum`], but also returns the datum contour —
    /// the `z = datum` crossing segments (plan view), i.e. the cut/fill outline.
    pub fn cut_fill_to_datum_detailed(&self, datum: f64) -> (CutFill, Vec<[[f64; 2]; 2]>) {
        let (mut cut, mut fill) = (0.0, 0.0);
        let mut contour = Vec::new();
        for &t in &self.triangles {
            let tri = [
                [
                    self.nodes[t[0]][0],
                    self.nodes[t[0]][1],
                    self.nodes[t[0]][2] - datum,
                ],
                [
                    self.nodes[t[1]][0],
                    self.nodes[t[1]][1],
                    self.nodes[t[1]][2] - datum,
                ],
                [
                    self.nodes[t[2]][0],
                    self.nodes[t[2]][1],
                    self.nodes[t[2]][2] - datum,
                ],
            ];
            let (cu, fi, seg) = tri_cut_fill_seg(&tri);
            cut += cu;
            fill += fi;
            if let Some(s) = seg {
                contour.push(s);
            }
        }
        (CutFill::from_cut_fill(cut, fill), contour)
    }

    /// TIN elevation at `(x, y)` by barycentric interpolation over the
    /// containing triangle, or `None` if `(x, y)` is outside every triangle.
    pub fn interpolate_z(&self, x: f64, y: f64) -> Option<f64> {
        for &t in &self.triangles {
            let (a, b, c) = (self.nodes[t[0]], self.nodes[t[1]], self.nodes[t[2]]);
            if let Some((_, _, _, z)) = barycentric_on_triangle(a, b, c, x, y) {
                return Some(z);
            }
        }
        None
    }

    /// Find the triangle containing `(x, y)` and return its index and
    /// barycentric coordinates. Returns `None` if outside every triangle.
    pub fn find_containing_triangle(&self, x: f64, y: f64) -> Option<(usize, (f64, f64, f64))> {
        for (idx, &t) in self.triangles.iter().enumerate() {
            let (a, b, c) = (self.nodes[t[0]], self.nodes[t[1]], self.nodes[t[2]]);
            if let Some((wa, wb, wc, _)) = barycentric_on_triangle(a, b, c, x, y) {
                return Some((idx, (wa, wb, wc)));
            }
        }
        None
    }

    /// Accelerated [`Surface::find_containing_triangle`] using a [`SpatialGrid`].
    ///
    /// Only the grid cell containing `(x, y)` plus its 8 neighbors are tested;
    /// a full-mesh scan is the fallback (e.g. a grid built from another
    /// surface), so the result is identical to the unindexed query.
    pub fn find_containing_triangle_fast(
        &self,
        grid: &SpatialGrid,
        x: f64,
        y: f64,
    ) -> Option<(usize, (f64, f64, f64))> {
        for idx in grid.query(x, y) {
            let Some(&t) = self.triangles.get(idx) else {
                continue;
            };
            let (a, b, c) = (self.nodes[t[0]], self.nodes[t[1]], self.nodes[t[2]]);
            if let Some((wa, wb, wc, _)) = barycentric_on_triangle(a, b, c, x, y) {
                return Some((idx, (wa, wb, wc)));
            }
        }
        self.find_containing_triangle(x, y)
    }

    /// Accelerated [`Surface::interpolate_z`] using a [`SpatialGrid`].
    /// Identical results to the unindexed query (same fallback contract).
    pub fn interpolate_z_fast(&self, grid: &SpatialGrid, x: f64, y: f64) -> Option<f64> {
        self.find_containing_triangle_fast(grid, x, y)
            .map(|(idx, (wa, wb, wc))| {
                let t = self.triangles[idx];
                wa * self.nodes[t[0]][2] + wb * self.nodes[t[1]][2] + wc * self.nodes[t[2]][2]
            })
    }

    /// Find all triangles whose bounding box overlaps a breakline's bbox
    /// (for efficient sync).
    pub fn find_affected_triangles(
        &self,
        breakline: &crate::featureline::FeatureLine,
    ) -> Vec<usize> {
        let mut affected = Vec::new();
        let bbox = breakline_bbox(breakline);

        for (idx, tri) in self.triangles.iter().enumerate() {
            let a = self.nodes[tri[0]];
            let b = self.nodes[tri[1]];
            let c = self.nodes[tri[2]];

            let tri_min_x = a[0].min(b[0]).min(c[0]);
            let tri_max_x = a[0].max(b[0]).max(c[0]);
            let tri_min_y = a[1].min(b[1]).min(c[1]);
            let tri_max_y = a[1].max(b[1]).max(c[1]);

            if !(tri_max_x < bbox.0
                || tri_min_x > bbox.1
                || tri_max_y < bbox.2
                || tri_min_y > bbox.3)
            {
                affected.push(idx);
            }
        }

        affected
    }

    /// Add a feature line as a constrained breakline to this surface.
    ///
    /// Thin method wrapper over [`crate::featureline::breakline::add_breakline`]
    /// so host-side code (e.g. `src/interactive.rs`) can call it as
    /// `surface.add_breakline(&feature_line, breakline_type)`.
    pub fn add_breakline(
        &mut self,
        breakline: &crate::featureline::entity::FeatureLine,
        breakline_type: crate::featureline::entity::BreaklineType,
    ) -> Result<crate::featureline::entity::BreaklineResult, String> {
        crate::featureline::breakline::add_breakline(self, breakline, breakline_type)
    }

    // ---------- Milestone 3: boundaries ----------

    /// Ray-casting point-in-polygon test. Returns true only when `pt` is
    /// strictly inside `poly` (points on an edge count as outside).
    /// Handles polygons with or without a duplicated closing vertex.
    fn point_in_polygon(pt: [f64; 2], poly: &[[f64; 2]]) -> bool {
        let n = poly.len();
        if n < 3 {
            return false;
        }
        // Ignore a duplicated closing vertex for the edge walk.
        let m = if (poly[0][0] - poly[n - 1][0]).abs() < 1e-12
            && (poly[0][1] - poly[n - 1][1]).abs() < 1e-12
        {
            n - 1
        } else {
            n
        };
        // On-edge -> strictly outside.
        for i in 0..m {
            let a = poly[i];
            let b = poly[(i + 1) % m];
            if point_on_segment(pt, a, b) {
                return false;
            }
        }
        // Ray casting (+x direction).
        let mut inside = false;
        let mut j = m - 1;
        for i in 0..m {
            let xi = poly[i][0];
            let yi = poly[i][1];
            let xj = poly[j][0];
            let yj = poly[j][1];
            if ((yi > pt[1]) != (yj > pt[1])) && (pt[0] < (xj - xi) * (pt[1] - yi) / (yj - yi) + xi)
            {
                inside = !inside;
            }
            j = i;
        }
        inside
    }

    /// Check if a triangle's 2-D centroid falls inside a polygon.
    fn triangle_centroid_in_polygon(tri: &Tri, nodes: &[Node], poly: &[[f64; 2]]) -> bool {
        let a = nodes[tri[0]];
        let b = nodes[tri[1]];
        let c = nodes[tri[2]];
        let centroid = [(a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0];
        Self::point_in_polygon(centroid, poly)
    }

    /// Count distinct vertices in a polygon (within 1e-9).
    fn distinct_vertex_count(polygon: &[[f64; 2]]) -> usize {
        let mut distinct: Vec<[f64; 2]> = Vec::new();
        for p in polygon {
            if !distinct
                .iter()
                .any(|q| (p[0] - q[0]).abs() < 1e-9 && (p[1] - q[1]).abs() < 1e-9)
            {
                distinct.push(*p);
            }
        }
        distinct.len()
    }

    /// Apply an outer boundary polygon: retain only triangles whose 2-D
    /// centroid falls strictly inside the polygon.
    pub fn apply_outer_boundary(&mut self, polygon: &[[f64; 2]]) -> Result<(), String> {
        if Self::distinct_vertex_count(polygon) < 3 {
            return Err("Boundary polygon must have at least 3 distinct vertices".into());
        }
        self.outer_boundary = Some(polygon.to_vec());
        self.triangles = std::mem::take(&mut self.triangles)
            .into_iter()
            .filter(|tri| Self::triangle_centroid_in_polygon(tri, &self.nodes, polygon))
            .collect();
        Ok(())
    }

    /// Add a hide boundary (interior hole): remove any triangle whose centroid
    /// falls inside the polygon. Multiple hide boundaries accumulate.
    pub fn add_hide_boundary(&mut self, polygon: &[[f64; 2]]) -> Result<(), String> {
        if Self::distinct_vertex_count(polygon) < 3 {
            return Err("Hide boundary must have at least 3 distinct vertices".into());
        }
        self.hide_boundaries.push(polygon.to_vec());
        self.triangles = std::mem::take(&mut self.triangles)
            .into_iter()
            .filter(|tri| !Self::triangle_centroid_in_polygon(tri, &self.nodes, polygon))
            .collect();
        Ok(())
    }

    // ---------- Milestone 4: general contours ----------

    /// Generate contour lines at multiples of `interval`.
    ///
    /// * `interval` — vertical spacing between contour levels.
    /// * `major_every` — every Nth level (by `k = Z / interval`) is major.
    /// * `smooth` — apply 2 iterations of Chaikin corner-cutting.
    pub fn generate_contours(
        &self,
        interval: f64,
        major_every: u32,
        smooth: bool,
    ) -> Vec<ContourLine> {
        if !(interval > 0.0)
            || !interval.is_finite()
            || self.nodes.is_empty()
            || self.triangles.is_empty()
        {
            return Vec::new();
        }
        let (z_min, z_max) = self
            .nodes
            .iter()
            .map(|n| n[2])
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(mn, mx), z| {
                (mn.min(z), mx.max(z))
            });
        if !(z_max > z_min) || !z_min.is_finite() {
            return Vec::new();
        }
        let major_every = major_every.max(1) as i64;
        let k_start = (z_min / interval).ceil() as i64;
        let k_end = (z_max / interval).floor() as i64;
        let mut out = Vec::new();
        for k in k_start..=k_end {
            let z = k as f64 * interval;
            // Clamp tiny floating residue so elevations read exactly.
            let ze = if (z - z_min).abs() < 1e-9 * interval.abs() {
                continue; // skip a level sitting exactly on the minimum node plane
            } else {
                z
            };
            let is_major = k.abs() % major_every == 0;
            let segments = self.contour_segments_at_elevation(ze);
            if segments.is_empty() {
                continue;
            }
            let polylines = Self::stitch_segments(segments);
            for mut poly in polylines {
                if smooth {
                    poly = Self::chaikin_smooth(&poly, 2);
                }
                // Force exact elevation (smoothing interpolates Z linearly so
                // this is a no-op numerically, but guarantees the contract).
                for p in &mut poly {
                    p[2] = z;
                }
                out.push(ContourLine {
                    elevation: z,
                    is_major,
                    points: poly,
                });
            }
        }
        out
    }

    /// Intersect every triangle with the horizontal plane `Z = z`.
    fn contour_segments_at_elevation(&self, z: f64) -> Vec<[[f64; 3]; 2]> {
        let mut segments = Vec::new();
        for tri in &self.triangles {
            let a = self.nodes[tri[0]];
            let b = self.nodes[tri[1]];
            let c = self.nodes[tri[2]];
            let (z1, z2, z3) = (a[2], b[2], c[2]);
            let (mn, mx) = (z1.min(z2).min(z3), z1.max(z2).max(z3));
            if z < mn - EPS || z > mx + EPS {
                continue;
            }
            // Coplanar triangle: no single contour line — skip.
            if (z1 - z).abs() < EPS && (z2 - z).abs() < EPS && (z3 - z).abs() < EPS {
                continue;
            }
            let edges = [(a, b), (b, c), (c, a)];
            let mut pts: Vec<[f64; 3]> = Vec::new();
            for (p1, p2) in edges {
                let h1 = p1[2] - z;
                let h2 = p2[2] - z;
                if h1 * h2 < 0.0 {
                    // Proper crossing.
                    let t = h1 / (h1 - h2);
                    pts.push([p1[0] + t * (p2[0] - p1[0]), p1[1] + t * (p2[1] - p1[1]), z]);
                } else if h1.abs() < EPS && h2.abs() >= EPS {
                    pts.push([p1[0], p1[1], z]);
                } else if h2.abs() < EPS && h1.abs() >= EPS {
                    pts.push([p2[0], p2[1], z]);
                }
                // Both on plane: edge lies in plane — ignore (handled by neighbours).
            }
            // Deduplicate (a vertex touch is found from two edges).
            let mut uniq: Vec<[f64; 3]> = Vec::new();
            for p in pts {
                if !uniq.iter().any(|q| (p[0] - q[0]).hypot(p[1] - q[1]) < 1e-9) {
                    uniq.push(p);
                }
            }
            if uniq.len() == 2 {
                segments.push([uniq[0], uniq[1]]);
            }
        }
        segments
    }

    /// Stitch disconnected segments into continuous polylines using an
    /// adjacency map keyed by 2-D coordinates rounded to 1 micron.
    fn stitch_segments(segments: Vec<[[f64; 3]; 2]>) -> Vec<Vec<[f64; 3]>> {
        use std::collections::HashMap;
        fn key(p: &[f64; 3]) -> (i64, i64) {
            (
                (p[0] * 1_000_000.0).round() as i64,
                (p[1] * 1_000_000.0).round() as i64,
            )
        }
        let mut adjacency: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        for (idx, seg) in segments.iter().enumerate() {
            adjacency.entry(key(&seg[0])).or_default().push(idx);
            adjacency.entry(key(&seg[1])).or_default().push(idx);
        }
        let mut used = vec![false; segments.len()];
        let mut polylines = Vec::new();
        for i in 0..segments.len() {
            if used[i] {
                continue;
            }
            used[i] = true;
            let mut poly = vec![segments[i][0], segments[i][1]];
            // Walk forward from the tail.
            loop {
                let k = key(&poly[poly.len() - 1]);
                let mut next: Option<(usize, [f64; 3])> = None;
                if let Some(cands) = adjacency.get(&k) {
                    for &ci in cands {
                        if used[ci] {
                            continue;
                        }
                        let s = &segments[ci];
                        if key(&s[0]) == k {
                            next = Some((ci, s[1]));
                            break;
                        } else if key(&s[1]) == k {
                            next = Some((ci, s[0]));
                            break;
                        }
                    }
                }
                match next {
                    Some((ci, pt)) => {
                        used[ci] = true;
                        poly.push(pt);
                    }
                    None => break,
                }
            }
            // Walk backward from the head.
            loop {
                let k = key(&poly[0]);
                let mut next: Option<(usize, [f64; 3])> = None;
                if let Some(cands) = adjacency.get(&k) {
                    for &ci in cands {
                        if used[ci] {
                            continue;
                        }
                        let s = &segments[ci];
                        if key(&s[1]) == k {
                            next = Some((ci, s[0]));
                            break;
                        } else if key(&s[0]) == k {
                            next = Some((ci, s[1]));
                            break;
                        }
                    }
                }
                match next {
                    Some((ci, pt)) => {
                        used[ci] = true;
                        poly.insert(0, pt);
                    }
                    None => break,
                }
            }
            if poly.len() >= 2 {
                polylines.push(poly);
            }
        }
        polylines
    }

    /// Chaikin corner-cutting smoothing.
    fn chaikin_smooth(points: &[[f64; 3]], iterations: u32) -> Vec<[f64; 3]> {
        if points.len() < 3 {
            return points.to_vec();
        }
        let closed = (points[0][0] - points[points.len() - 1][0])
            .hypot(points[0][1] - points[points.len() - 1][1])
            < 1e-9;
        let mut pts = points.to_vec();
        for _ in 0..iterations {
            if closed {
                let mut next = Vec::with_capacity(pts.len() * 2);
                for i in 0..pts.len() {
                    let p = pts[i];
                    let q = pts[(i + 1) % pts.len()];
                    next.push([
                        0.75 * p[0] + 0.25 * q[0],
                        0.75 * p[1] + 0.25 * q[1],
                        0.75 * p[2] + 0.25 * q[2],
                    ]);
                    next.push([
                        0.25 * p[0] + 0.75 * q[0],
                        0.25 * p[1] + 0.75 * q[1],
                        0.25 * p[2] + 0.75 * q[2],
                    ]);
                }
                pts = next;
            } else {
                let mut next = Vec::with_capacity(pts.len() * 2);
                next.push(pts[0]);
                for w in pts.windows(2) {
                    let (p, q) = (w[0], w[1]);
                    next.push([
                        0.75 * p[0] + 0.25 * q[0],
                        0.75 * p[1] + 0.25 * q[1],
                        0.75 * p[2] + 0.25 * q[2],
                    ]);
                    next.push([
                        0.25 * p[0] + 0.75 * q[0],
                        0.25 * p[1] + 0.75 * q[1],
                        0.25 * p[2] + 0.75 * q[2],
                    ]);
                }
                next.push(*pts.last().unwrap());
                pts = next;
            }
        }
        pts
    }
}

/// Barycentric weights and interpolated Z of `(x, y)` on triangle `(a, b, c)`.
///
/// Returns `None` for degenerate triangles or points outside, keeping the
/// `-1e-9` boundary-edge tolerance shared by all point queries.
fn barycentric_on_triangle(
    a: Node,
    b: Node,
    c: Node,
    x: f64,
    y: f64,
) -> Option<(f64, f64, f64, f64)> {
    let d = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
    if d.abs() < EPS {
        return None;
    }
    let wa = ((b[1] - c[1]) * (x - c[0]) + (c[0] - b[0]) * (y - c[1])) / d;
    let wb = ((c[1] - a[1]) * (x - c[0]) + (a[0] - c[0]) * (y - c[1])) / d;
    let wc = 1.0 - wa - wb;
    if wa >= -1e-9 && wb >= -1e-9 && wc >= -1e-9 {
        Some((wa, wb, wc, wa * a[2] + wb * b[2] + wc * c[2]))
    } else {
        None
    }
}

/// Uniform 2-D spatial hash grid over triangle bounding boxes.
///
/// Built once per surface, then reused across thousands of point queries
/// (daylight ray-casting, contour sampling). Querying tests only the cell
/// containing the point plus its 8 neighbors instead of the full mesh.
#[derive(Debug, Clone, PartialEq)]
pub struct SpatialGrid {
    cell_size: f64,
    min_x: f64,
    min_y: f64,
    cells: std::collections::HashMap<(i32, i32), Vec<usize>>,
}

impl SpatialGrid {
    /// Index every triangle of `surface` into the cells overlapped by its
    /// bounding box. A non-positive or non-finite `cell_size` falls back to
    /// `max_extent / sqrt(triangle_count)`.
    pub fn build(surface: &Surface, cell_size: f64) -> Self {
        let mut grid = SpatialGrid {
            cell_size: 1.0,
            min_x: 0.0,
            min_y: 0.0,
            cells: std::collections::HashMap::new(),
        };
        if surface.nodes.is_empty() || surface.triangles.is_empty() {
            return grid;
        }
        let (minx, maxx, miny, maxy) = surface.extent();
        if !minx.is_finite() || !miny.is_finite() {
            return grid;
        }
        let n = surface.triangles.len().max(1) as f64;
        let auto = ((maxx - minx).max(maxy - miny) / n.sqrt()).max(1e-9);
        let cs = if cell_size.is_finite() && cell_size > 0.0 {
            cell_size
        } else {
            auto
        };
        grid.cell_size = cs;
        grid.min_x = minx;
        grid.min_y = miny;
        for (idx, &t) in surface.triangles.iter().enumerate() {
            let (a, b, c) = (
                surface.nodes[t[0]],
                surface.nodes[t[1]],
                surface.nodes[t[2]],
            );
            let (x0, x1) = (a[0].min(b[0]).min(c[0]), a[0].max(b[0]).max(c[0]));
            let (y0, y1) = (a[1].min(b[1]).min(c[1]), a[1].max(b[1]).max(c[1]));
            let (cx0, cx1) = (grid.cell_of_x(x0), grid.cell_of_x(x1));
            let (cy0, cy1) = (grid.cell_of_y(y0), grid.cell_of_y(y1));
            for cx in cx0..=cx1 {
                for cy in cy0..=cy1 {
                    grid.cells.entry((cx, cy)).or_default().push(idx);
                }
            }
        }
        grid
    }

    fn cell_of_x(&self, x: f64) -> i32 {
        ((x - self.min_x) / self.cell_size).floor() as i32
    }

    fn cell_of_y(&self, y: f64) -> i32 {
        ((y - self.min_y) / self.cell_size).floor() as i32
    }

    /// Triangle indices in the point's cell plus its 8 neighbors
    /// (deduplicated and sorted in ascending index order).
    ///
    /// The ascending order is what makes the accelerated lookup agree with
    /// [`Surface::find_containing_triangle`], which returns the lowest-index
    /// containing triangle: cell traversal order alone can surface a
    /// higher-index neighbour first on a shared edge.
    pub fn query(&self, x: f64, y: f64) -> Vec<usize> {
        let mut out = Vec::new();
        self.query_into(x, y, &mut out);
        out
    }

    /// [`SpatialGrid::query`] without allocating: appends into `out`.
    ///
    /// Entries already present in `out` are left untouched; only the newly
    /// appended candidates are deduplicated and sorted ascending, so callers
    /// can keep reusing the same buffer across queries.
    pub fn query_into(&self, x: f64, y: f64, out: &mut Vec<usize>) {
        if self.cells.is_empty() {
            return;
        }
        let start = out.len();
        let cx = self.cell_of_x(x);
        let cy = self.cell_of_y(y);
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(ids) = self.cells.get(&(cx + dx, cy + dy)) {
                    for &id in ids {
                        if !out.contains(&id) {
                            out.push(id);
                        }
                    }
                }
            }
        }
        // Sort the freshly appended tail ascending so the caller sees candidates
        // in mesh order regardless of cell traversal order, then compact out
        // any duplicate. Done in place: no allocation, and `out[start..]` keeps
        // the caller's pre-existing entries exactly as they were.
        out[start..].sort_unstable();
        let mut write = start;
        for read in start..out.len() {
            if write == start || out[read] != out[write - 1] {
                out[write] = out[read];
                write += 1;
            }
        }
        out.truncate(write);
    }

    /// Total cell entries (a triangle spanning several cells counts once per
    /// cell; for diagnostics and tests).
    pub fn cell_entry_count(&self) -> usize {
        self.cells.values().map(|v| v.len()).sum()
    }
}

fn point_on_segment(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> bool {
    let cross = (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
    if cross.abs() > 1e-9 {
        return false;
    }
    let dot = (p[0] - a[0]) * (p[0] - b[0]) + (p[1] - a[1]) * (p[1] - b[1]);
    dot <= 1e-9
}

/// Bounding box `(min_x, max_x, min_y, max_y)` of a feature line.
fn breakline_bbox(breakline: &crate::featureline::FeatureLine) -> (f64, f64, f64, f64) {
    let mut minx = f64::INFINITY;
    let mut maxx = f64::NEG_INFINITY;
    let mut miny = f64::INFINITY;
    let mut maxy = f64::NEG_INFINITY;
    for v in &breakline.vertices {
        minx = minx.min(v.pt.x);
        maxx = maxx.max(v.pt.x);
        miny = miny.min(v.pt.y);
        maxy = maxy.max(v.pt.y);
    }
    (minx, maxx, miny, maxy)
}

// --- Delaunay triangulation (Bowyer-Watson) ---

fn orient2d(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

fn in_circumcircle(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let orient = orient2d(a, b, c);
    if orient.abs() < 1e-12 {
        return false;
    }
    let adx = a[0] - d[0];
    let ady = a[1] - d[1];
    let bdx = b[0] - d[0];
    let bdy = b[1] - d[1];
    let cdx = c[0] - d[0];
    let cdy = c[1] - d[1];
    let det = adx * (bdy * (cdx * cdx + cdy * cdy) - cdy * (bdx * bdx + bdy * bdy))
        - ady * (bdx * (cdx * cdx + cdy * cdy) - cdx * (bdx * bdx + bdy * bdy))
        + (adx * adx + ady * ady) * (bdx * cdy - bdy * cdx);
    if orient > 0.0 {
        det > 1e-12
    } else {
        det < -1e-12
    }
}

/// Delaunay triangulation of 2-D points (Bowyer–Watson, O(n^2)).
/// Returns index triples into `points`. Points fewer than 3 yield no triangles.
fn delaunay(points: &[[f64; 2]]) -> Vec<Tri> {
    let n = points.len();
    if n < 3 {
        return Vec::new();
    }
    // Bounding box + super-triangle.
    let (mut minx, mut maxx, mut miny, mut maxy) = (
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
    );
    for p in points {
        minx = minx.min(p[0]);
        maxx = maxx.max(p[0]);
        miny = miny.min(p[1]);
        maxy = maxy.max(p[1]);
    }
    let dx = (maxx - minx).max(1.0);
    let dy = (maxy - miny).max(1.0);
    let d = dx.max(dy) * 100.0;
    let midx = (minx + maxx) / 2.0;
    let midy = (miny + maxy) / 2.0;
    let mut verts: Vec<[f64; 2]> = points.to_vec();
    let s0 = verts.len();
    verts.push([midx - 2.0 * d, midy - d]);
    verts.push([midx, midy + 2.0 * d]);
    verts.push([midx + 2.0 * d, midy - d]);

    let mut tris: Vec<[usize; 3]> = vec![[s0, s0 + 1, s0 + 2]];

    for idx in 0..n {
        let p = points[idx];
        // Skip exact duplicates of an earlier point (keeps the node stored
        // but unreferenced rather than producing degenerate triangles).
        if points[..idx]
            .iter()
            .any(|q| (p[0] - q[0]).abs() < 1e-12 && (p[1] - q[1]).abs() < 1e-12)
        {
            continue;
        }
        let mut bad: Vec<usize> = Vec::new();
        for (ti, &t) in tris.iter().enumerate() {
            if in_circumcircle(verts[t[0]], verts[t[1]], verts[t[2]], p) {
                bad.push(ti);
            }
        }
        if bad.is_empty() {
            continue;
        }
        // Boundary of the polygonal hole: edges used exactly once.
        // BTreeMap (not HashMap) so hole-boundary iteration — and therefore the
        // order triangles are appended — is deterministic across runs.
        use std::collections::BTreeMap;
        let mut edge_count: BTreeMap<(usize, usize), usize> = BTreeMap::new();
        for &bi in &bad {
            let t = tris[bi];
            for e in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                let key = if e.0 < e.1 { (e.0, e.1) } else { (e.1, e.0) };
                *edge_count.entry(key).or_insert(0) += 1;
            }
        }
        let bad_set: std::collections::HashSet<usize> = bad.into_iter().collect();
        let mut kept = Vec::with_capacity(tris.len());
        for (ti, t) in tris.into_iter().enumerate() {
            if !bad_set.contains(&ti) {
                kept.push(t);
            }
        }
        tris = kept;
        for (e, c) in edge_count {
            if c == 1 {
                // Orient CCW with respect to p.
                let (a, b) = e;
                if orient2d(verts[a], verts[b], p) > 0.0 {
                    tris.push([a, b, idx]);
                } else {
                    tris.push([b, a, idx]);
                }
            }
        }
    }

    // Drop triangles touching the super-triangle, enforce CCW.
    tris.into_iter()
        .filter(|t| t[0] < n && t[1] < n && t[2] < n)
        .map(|t| {
            let (a, b, c) = (verts[t[0]], verts[t[1]], verts[t[2]]);
            if orient2d(a, b, c) < 0.0 {
                [t[0], t[2], t[1]]
            } else {
                t
            }
        })
        .filter(|t| orient2d(verts[t[0]], verts[t[1]], verts[t[2]]).abs() > 1e-12)
        .collect()
}

// --- Exact per-triangle datum cut/fill ---

/// Exact cut/fill of one triangle (with heights `h = z - datum`) plus the
/// datum crossing segment (plan view) when the triangle straddles the datum.
fn tri_cut_fill_seg(tri: &[[f64; 3]; 3]) -> (f64, f64, Option<[[f64; 2]; 2]>) {
    let area = ((tri[1][0] - tri[0][0]) * (tri[2][1] - tri[0][1])
        - (tri[2][0] - tri[0][0]) * (tri[1][1] - tri[0][1]))
        .abs()
        / 2.0;
    if area < EPS {
        return (0.0, 0.0, None);
    }
    let h = [tri[0][2], tri[1][2], tri[2][2]];
    let above = h.iter().filter(|v| **v > EPS).count();
    let below = h.iter().filter(|v| **v < -EPS).count();
    let mean = (h[0] + h[1] + h[2]) / 3.0;
    if above == 3 || (above > 0 && below == 0) {
        return (area * mean, 0.0, None);
    }
    if below == 3 || (below > 0 && above == 0) {
        return (0.0, -area * mean, None);
    }
    // Straddling: find the two crossing points on edges with sign change.
    let mut pts: Vec<[f64; 2]> = Vec::new();
    for e in [(0usize, 1usize), (1, 2), (2, 0)] {
        let (i, j) = e;
        if h[i] * h[j] < 0.0 {
            let t = h[i] / (h[i] - h[j]);
            pts.push([
                tri[i][0] + t * (tri[j][0] - tri[i][0]),
                tri[i][1] + t * (tri[j][1] - tri[i][1]),
            ]);
        } else if h[i].abs() < EPS && h[j].abs() >= EPS {
            pts.push([tri[i][0], tri[i][1]]);
        } else if h[j].abs() < EPS && h[i].abs() >= EPS {
            pts.push([tri[j][0], tri[j][1]]);
        }
    }
    // Deduplicate.
    let mut uniq: Vec<[f64; 2]> = Vec::new();
    for p in pts {
        if !uniq
            .iter()
            .any(|q| (p[0] - q[0]).hypot(p[1] - q[1]) < 1e-12)
        {
            uniq.push(p);
        }
    }
    let seg = if uniq.len() == 2 {
        Some([[uniq[0][0], uniq[0][1]], [uniq[1][0], uniq[1][1]]])
    } else {
        None
    };
    // Exact split: subdivide the triangle by the zero line. Classify each
    // corner; integrate above/below parts via fan triangulation of the
    // above-polygon and below-polygon in the triangle's plane projection.
    // Simpler exact route: volume = sum over the 3 sub-triangles formed with
    // the centroid? No — instead clip the triangle (as an XY polygon with
    // heights) against h >= 0 and h <= 0 half-planes.
    let poly: Vec<([f64; 2], f64)> = tri.iter().map(|p| ([p[0], p[1]], p[2])).collect();
    let above_poly = clip_polygon_by_height(&poly, true);
    let below_poly = clip_polygon_by_height(&poly, false);
    // polygon_height_volume returns the magnitude (|integral|); the clipped
    // polys are single-signed so this is the exact above/below contribution.
    let cut = polygon_height_volume(&above_poly);
    let fill = polygon_height_volume(&below_poly);
    (cut.max(0.0), fill.max(0.0), seg)
}

/// Clip a polygon (XY + height per vertex) against the half-plane h>=0
/// (`keep_above=true`) or h<=0.
fn clip_polygon_by_height(poly: &[([f64; 2], f64)], keep_above: bool) -> Vec<([f64; 2], f64)> {
    if poly.is_empty() {
        return Vec::new();
    }
    let inside = |h: f64| {
        if keep_above {
            h >= -EPS
        } else {
            h <= EPS
        }
    };
    let mut out: Vec<([f64; 2], f64)> = Vec::new();
    let n = poly.len();
    for i in 0..n {
        let (p_cur, h_cur) = poly[i];
        let (p_prev, h_prev) = poly[(i + n - 1) % n];
        let cur_in = inside(h_cur);
        let prev_in = inside(h_prev);
        if cur_in {
            if !prev_in {
                let t = h_prev / (h_prev - h_cur);
                out.push((
                    [
                        p_prev[0] + t * (p_cur[0] - p_prev[0]),
                        p_prev[1] + t * (p_cur[1] - p_prev[1]),
                    ],
                    0.0,
                ));
            }
            out.push((p_cur, h_cur));
        } else if prev_in {
            let t = h_prev / (h_prev - h_cur);
            out.push((
                [
                    p_prev[0] + t * (p_cur[0] - p_prev[0]),
                    p_prev[1] + t * (p_cur[1] - p_prev[1]),
                ],
                0.0,
            ));
        }
    }
    out
}

/// Exact integral of a linear height field over an XY polygon:
/// fan-triangulate from vertex 0, sum area*mean_h.
fn polygon_height_volume(poly: &[([f64; 2], f64)]) -> f64 {
    if poly.len() < 3 {
        return 0.0;
    }
    let mut vol = 0.0;
    for i in 1..poly.len() - 1 {
        let (a, ha) = poly[0];
        let (b, hb) = poly[i];
        let (c, hc) = poly[i + 1];
        let area = ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])) / 2.0;
        vol += area * (ha + hb + hc) / 3.0;
    }
    vol.abs()
}

// --- Two-surface volumes ---

/// Exact TIN-overlay cut/fill between two independent surfaces.
/// Clips each top facet against each bottom facet (O(Nt*Nb)); on every convex
/// overlay cell both surfaces are linear so `dz` is linear and integrates
/// exactly, split at `dz = 0`.
pub fn composite_cut_fill_detailed(top: &Surface, bottom: &Surface) -> CompositeDetail {
    let (mut cut, mut fill) = (0.0, 0.0);
    let mut line: Vec<[[f64; 2]; 2]> = Vec::new();
    // Bounding boxes for a cheap pre-check.
    let top_boxes: Vec<(f64, f64, f64, f64)> =
        top.triangles.iter().map(|&t| tri_bbox(top, t)).collect();
    let bot_boxes: Vec<(f64, f64, f64, f64)> = bottom
        .triangles
        .iter()
        .map(|&t| tri_bbox(bottom, t))
        .collect();
    for (ti, &tt) in top.triangles.iter().enumerate() {
        let tp = [top.nodes[tt[0]], top.nodes[tt[1]], top.nodes[tt[2]]];
        let tb = top_boxes[ti];
        for (bi, &bt) in bottom.triangles.iter().enumerate() {
            let bb = bot_boxes[bi];
            if tb.1 < bb.0 || tb.0 > bb.1 || tb.3 < bb.2 || tb.2 > bb.3 {
                continue;
            }
            let bp = [
                bottom.nodes[bt[0]],
                bottom.nodes[bt[1]],
                bottom.nodes[bt[2]],
            ];
            let subject = vec![
                [tp[0][0], tp[0][1]],
                [tp[1][0], tp[1][1]],
                [tp[2][0], tp[2][1]],
            ];
            let clip = vec![
                [bp[0][0], bp[0][1]],
                [bp[1][0], bp[1][1]],
                [bp[2][0], bp[2][1]],
            ];
            let overlap = clip_polygon(&subject, &clip);
            if overlap.len() < 3 || polygon_area(&overlap).abs() < EPS {
                continue;
            }
            // dz at each overlap vertex. Use plane extrapolation (not a
            // containment query): overlap vertices lie on both facet planes
            // up to rounding noise, and a containment miss must never
            // collapse Z to 0.
            let mut dz_poly: Vec<([f64; 2], f64)> = Vec::new();
            for p in &overlap {
                let zt = plane_z(&tp, p[0], p[1]);
                let zb = plane_z(&bp, p[0], p[1]);
                dz_poly.push((*p, zt - zb));
            }
            let above = clip_polygon_by_height(&dz_poly, true);
            let below = clip_polygon_by_height(&dz_poly, false);
            cut += polygon_height_volume(&above);
            fill += polygon_height_volume(&below);
            // Zero-crossing segments inside this cell.
            let mut zeros: Vec<[f64; 2]> = Vec::new();
            for i in 0..dz_poly.len() {
                let (p1, h1) = dz_poly[i];
                let (p2, h2) = dz_poly[(i + 1) % dz_poly.len()];
                if h1 * h2 < 0.0 {
                    let t = h1 / (h1 - h2);
                    zeros.push([p1[0] + t * (p2[0] - p1[0]), p1[1] + t * (p2[1] - p1[1])]);
                }
            }
            if zeros.len() == 2 {
                line.push([[zeros[0][0], zeros[0][1]], [zeros[1][0], zeros[1][1]]]);
            }
        }
    }
    CompositeDetail {
        cut_fill: CutFill::from_cut_fill(cut, fill),
        cutfill_line: line,
    }
}

/// Exact composite cut/fill (cut/fill/net only).
pub fn exact_composite_cut_fill(top: &Surface, bottom: &Surface) -> CutFill {
    composite_cut_fill_detailed(top, bottom).cut_fill
}

/// Grid (column) volume between two independent surfaces. Samples the cell
/// centre of a `step` x `step` grid over the overlapping extent; cells where
/// either surface is undefined are skipped. Accuracy depends on `grid_step`.
pub fn grid_cut_fill(top: &Surface, bottom: &Surface, step: f64) -> GridVolume {
    let mut out = GridVolume {
        cut: 0.0,
        fill: 0.0,
        net: 0.0,
        plan_area: 0.0,
        n_cells: 0,
    };
    if !(step > 0.0) || !step.is_finite() {
        return out;
    }
    let (t0, t1, t2, t3) = top.extent();
    let (b0, b1, b2, b3) = bottom.extent();
    let (x0, x1) = (t0.max(b0), t1.min(b1));
    let (y0, y1) = (t2.max(b2), t3.min(b3));
    if x1 <= x0 || y1 <= y0 {
        return out;
    }
    let nx = ((x1 - x0) / step).ceil() as usize;
    let ny = ((y1 - y0) / step).ceil() as usize;
    for ix in 0..nx {
        for iy in 0..ny {
            let x = x0 + (ix as f64 + 0.5) * step;
            let y = y0 + (iy as f64 + 0.5) * step;
            if x > x1 || y > y1 {
                continue;
            }
            match (top.interpolate_z(x, y), bottom.interpolate_z(x, y)) {
                (Some(zt), Some(zb)) => {
                    let dz = zt - zb;
                    if dz >= 0.0 {
                        out.cut += dz * step * step;
                    } else {
                        out.fill += -dz * step * step;
                    }
                    out.plan_area += step * step;
                    out.n_cells += 1;
                }
                _ => {}
            }
        }
    }
    out.net = out.cut - out.fill;
    out
}

fn tri_bbox(s: &Surface, t: Tri) -> (f64, f64, f64, f64) {
    let (a, b, c) = (s.nodes[t[0]], s.nodes[t[1]], s.nodes[t[2]]);
    (
        a[0].min(b[0]).min(c[0]),
        a[0].max(b[0]).max(c[0]),
        a[1].min(b[1]).min(c[1]),
        a[1].max(b[1]).max(c[1]),
    )
}

fn polygon_area(poly: &[[f64; 2]]) -> f64 {
    let mut a = 0.0;
    for i in 0..poly.len() {
        let p = poly[i];
        let q = poly[(i + 1) % poly.len()];
        a += p[0] * q[1] - q[0] * p[1];
    }
    a / 2.0
}

/// Sutherland–Hodgman clip of a subject polygon against a convex clip polygon
/// (both as XY rings; clip is assumed CCW-or-CW convex, e.g. a triangle).
fn clip_polygon(subject: &[[f64; 2]], clip: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let mut out = subject.to_vec();
    if out.is_empty() || clip.len() < 3 {
        return Vec::new();
    }
    let clip_ccw = polygon_area(clip) > 0.0;
    for i in 0..clip.len() {
        let a = clip[i];
        let b = clip[(i + 1) % clip.len()];
        let mut input = std::mem::take(&mut out);
        if input.is_empty() {
            break;
        }
        let mut prev = *input.last().unwrap();
        for cur in input.drain(..) {
            let cur_in = inside_half_plane(cur, a, b, clip_ccw);
            let prev_in = inside_half_plane(prev, a, b, clip_ccw);
            if cur_in {
                if !prev_in {
                    out.push(line_intersection(prev, cur, a, b));
                }
                out.push(cur);
            } else if prev_in {
                out.push(line_intersection(prev, cur, a, b));
            }
            prev = cur;
        }
    }
    out
}

fn inside_half_plane(p: [f64; 2], a: [f64; 2], b: [f64; 2], ccw: bool) -> bool {
    let cross = (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
    if ccw {
        cross >= -1e-12
    } else {
        cross <= 1e-12
    }
}

fn line_intersection(p1: [f64; 2], p2: [f64; 2], a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    let d = (p2[0] - p1[0]) * (b[1] - a[1]) - (p2[1] - p1[1]) * (b[0] - a[0]);
    if d.abs() < 1e-18 {
        return [(p1[0] + p2[0]) / 2.0, (p1[1] + p2[1]) / 2.0];
    }
    let t = ((a[0] - p1[0]) * (b[1] - a[1]) - (a[1] - p1[1]) * (b[0] - a[0])) / d;
    [p1[0] + t * (p2[0] - p1[0]), p1[1] + t * (p2[1] - p1[1])]
}

/// Plane-extrapolated Z of a 3-D triangle's supporting plane at `(x, y)`.
/// Always returns a value (mean Z for degenerate triangles); overlap vertices
/// are on the plane up to rounding noise, so extrapolation is exact there.
fn plane_z(tri: &[Node; 3], x: f64, y: f64) -> f64 {
    let (a, b, c) = (tri[0], tri[1], tri[2]);
    let d = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
    if d.abs() < EPS {
        return (a[2] + b[2] + c[2]) / 3.0;
    }
    let wa = ((b[1] - c[1]) * (x - c[0]) + (c[0] - b[0]) * (y - c[1])) / d;
    let wb = ((c[1] - a[1]) * (x - c[0]) + (a[0] - c[0]) * (y - c[1])) / d;
    let wc = 1.0 - wa - wb;
    wa * a[2] + wb * b[2] + wc * c[2]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid_surface(nx: usize, ny: usize, size: f64, z: impl Fn(f64, f64) -> f64) -> Surface {
        let mut pts = Vec::new();
        for iy in 0..=ny {
            for ix in 0..=nx {
                let x = ix as f64 / nx as f64 * size;
                let y = iy as f64 / ny as f64 * size;
                pts.push([x, y, z(x, y)]);
            }
        }
        Surface::from_points(&pts)
    }

    #[test]
    fn square_gives_two_triangles() {
        let s = Surface::from_points(&[
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ]);
        assert_eq!(s.triangles.len(), 2);
        assert!((s.area_2d() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn from_points_is_deterministic_across_calls() {
        // Delaunay hole-boundary edge collection must not depend on hash
        // iteration order: triangle order feeds `find_containing_triangle`'s
        // lowest-index tie-break, so a varying order silently changes which
        // facet wins on shared edges.
        let build = || grid_surface(12, 9, 40.0, |x, y| 0.3 * x - 0.2 * y).triangles;
        let first = build();
        assert!(!first.is_empty());
        for _ in 0..16 {
            assert_eq!(build(), first);
        }
    }

    #[test]
    fn outer_boundary_clips_to_window() {
        // Flat 100x100 surface.
        let s = grid_surface(10, 10, 100.0, |_, _| 10.0);
        let mut bounded = s.clone();
        let outer = vec![
            [10.0, 10.0],
            [90.0, 10.0],
            [90.0, 90.0],
            [10.0, 90.0],
            [10.0, 10.0],
        ];
        bounded.apply_outer_boundary(&outer).unwrap();
        assert!(!bounded.triangles.is_empty());
        assert!(bounded.triangles.len() < s.triangles.len());
        // All remaining centroids strictly inside 10..90.
        for t in &bounded.triangles {
            let (a, b, c) = (
                bounded.nodes[t[0]],
                bounded.nodes[t[1]],
                bounded.nodes[t[2]],
            );
            let cx = (a[0] + b[0] + c[0]) / 3.0;
            let cy = (a[1] + b[1] + c[1]) / 3.0;
            assert!(
                cx > 10.0 && cx < 90.0 && cy > 10.0 && cy < 90.0,
                "centroid {cx},{cy} outside"
            );
        }
        // Rejects degenerate polygons.
        assert!(bounded
            .apply_outer_boundary(&[[0.0, 0.0], [1.0, 1.0]])
            .is_err());
    }

    #[test]
    fn hide_boundary_cuts_hole() {
        let s = grid_surface(10, 10, 100.0, |_, _| 10.0);
        let mut holed = s.clone();
        let hole = vec![
            [40.0, 40.0],
            [60.0, 40.0],
            [60.0, 60.0],
            [40.0, 60.0],
            [40.0, 40.0],
        ];
        holed.add_hide_boundary(&hole).unwrap();
        assert!(!holed.triangles.is_empty());
        for t in &holed.triangles {
            let (a, b, c) = (holed.nodes[t[0]], holed.nodes[t[1]], holed.nodes[t[2]]);
            let cx = (a[0] + b[0] + c[0]) / 3.0;
            let cy = (a[1] + b[1] + c[1]) / 3.0;
            let inside = cx > 40.0 && cx < 60.0 && cy > 40.0 && cy < 60.0;
            assert!(!inside, "centroid {cx},{cy} inside hole");
        }
    }

    #[test]
    fn contours_on_pyramid_have_correct_z_and_loops() {
        // Pyramid: 4 corners at 0, apex at 10.
        let s = Surface::from_points(&[
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [10.0, 10.0, 0.0],
            [0.0, 10.0, 0.0],
            [5.0, 5.0, 10.0],
        ]);
        let contours = s.generate_contours(1.0, 5, false);
        assert!(!contours.is_empty());
        for c in &contours {
            // Elevation is an exact multiple of the interval.
            assert!((c.elevation / 1.0).round() * 1.0 - c.elevation == 0.0);
            assert!(c.points.len() >= 2);
            for p in &c.points {
                assert!((p[2] - c.elevation).abs() < 1e-9);
            }
            let k = (c.elevation / 1.0).round() as i64;
            assert_eq!(c.is_major, k.abs() % 5 == 0);
        }
        // Smoothing keeps elevations and count.
        let smooth = s.generate_contours(1.0, 5, true);
        assert_eq!(smooth.len(), contours.len());
    }

    #[test]
    fn spatial_grid_matches_full_scan_on_golden_surface() {
        use std::time::Instant;
        // The 1,770-point / 3,234-triangle Civil 3D fixture: the largest mesh
        // in the repo and the realistic stress case for daylight ray-casting.
        let xml = include_str!("../tests/fixtures/road_surface.landxml");
        let surf = crate::landxml::read_first_surface(xml)
            .expect("golden fixture parses")
            .surface;
        assert!(surf.triangles.len() >= 3000);

        let (minx, maxx, miny, maxy) = surf.extent();
        let n = surf.triangles.len() as f64;
        let grid = SpatialGrid::build(&surf, (maxx - minx).max(maxy - miny) / n.sqrt());
        assert!(grid.cell_entry_count() >= surf.triangles.len());

        // Corpus: every node, every triangle centroid, plus out-of-bounds.
        let mut queries: Vec<[f64; 2]> =
            Vec::with_capacity(surf.nodes.len() + surf.triangles.len() + 3);
        for nd in &surf.nodes {
            queries.push([nd[0], nd[1]]);
        }
        for t in &surf.triangles {
            let (a, b, c) = (surf.nodes[t[0]], surf.nodes[t[1]], surf.nodes[t[2]]);
            queries.push([(a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0]);
        }
        let dx = (maxx - minx).max(1.0);
        let dy = (maxy - miny).max(1.0);
        queries.push([minx - dx, miny - dy]);
        queries.push([maxx + dx, maxy + dy]);
        queries.push([(minx + maxx) / 2.0, maxy + dy]);

        let t0 = Instant::now();
        let slow: Vec<Option<f64>> = queries
            .iter()
            .map(|q| surf.interpolate_z(q[0], q[1]))
            .collect();
        let slow_time = t0.elapsed();
        let t1 = Instant::now();
        let fast: Vec<Option<f64>> = queries
            .iter()
            .map(|q| surf.interpolate_z_fast(&grid, q[0], q[1]))
            .collect();
        let fast_time = t1.elapsed();
        // Bit-identical: the grid always contains the true container in the
        // point's own cell, so the first hit matches the full scan's.
        assert_eq!(slow, fast);

        let cand_total: usize = queries.iter().map(|q| grid.query(q[0], q[1]).len()).sum();
        let avg_cand = cand_total as f64 / queries.len() as f64;
        eprintln!(
            "spatial_grid: {} queries on {} tris — full scan {:?}, grid {:?}, avg {:.1} candidates/query",
            queries.len(),
            surf.triangles.len(),
            slow_time,
            fast_time,
            avg_cand,
        );
        // Order-of-magnitude search-space reduction (no hard timing assert:
        // wall-clock is flaky under load; correctness above is exact).
        assert!(
            avg_cand < surf.triangles.len() as f64 / 5.0,
            "avg {avg_cand:.1} candidates on {} tris",
            surf.triangles.len()
        );
    }

    /// Two triangles sharing the vertical edge `x = 1`, split across the cell
    /// boundary that `cell_size = 1.0` puts there. Querying a point on that
    /// shared edge reaches *both* triangles, so the accelerated lookup is only
    /// equivalent to the full scan if candidates are visited in ascending
    /// index order — cell traversal order alone would visit the left cell
    /// (triangle 1) before the right cell (triangle 0) and return triangle 1.
    #[test]
    fn spatial_grid_prefers_lowest_index_on_shared_edge() {
        let surf = Surface {
            name: "shared-edge".to_string(),
            // Nodes 0 and 1 are the shared edge; 2 and 3 are the apices on
            // opposite sides of it. Elevations differ so the triangles are
            // distinct surfaces.
            nodes: vec![
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 10.0],
                [2.0, 0.0, 0.0],
                [0.0, 0.0, 0.0],
            ],
            triangles: vec![[0, 1, 2], [0, 1, 3]],
            outer_boundary: None,
            hide_boundaries: Vec::new(),
        };
        let grid = SpatialGrid::build(&surf, 1.0);

        // A point exactly on the shared edge is inside both triangles.
        let (ex, ey) = (1.0, 0.5);
        let slow = surf.find_containing_triangle(ex, ey);
        let fast = surf.find_containing_triangle_fast(&grid, ex, ey);
        assert_eq!(
            slow.map(|(i, _)| i),
            Some(0),
            "full scan takes lowest index"
        );
        assert_eq!(fast, slow, "accelerated lookup must match the full scan");

        // Elevation cannot discriminate here: both triangles interpolate the
        // same z along the shared edge, so the triangle *index* is the only
        // observable that proves the ordering fix.
        assert_eq!(
            surf.interpolate_z_fast(&grid, ex, ey),
            surf.interpolate_z(ex, ey)
        );

        // Cell traversal visits triangle 1's cell first, so the raw candidate
        // order used to be descending: it must now come back sorted ascending.
        assert_eq!(grid.query(ex, ey), vec![0, 1]);

        // Reuse contract: existing buffer entries are preserved untouched and
        // only the newly appended candidates are sorted/deduplicated.
        let mut out = vec![7usize, 5];
        grid.query_into(ex, ey, &mut out);
        assert_eq!(out, vec![7, 5, 0, 1]);

        // The whole shared edge, plus interior points of both triangles.
        for t in 0..=10 {
            let f = t as f64 / 10.0;
            let y = f;
            assert_eq!(
                surf.find_containing_triangle_fast(&grid, 1.0, y),
                surf.find_containing_triangle(1.0, y),
                "shared edge mismatch at y={y}"
            );
        }
        assert_eq!(
            surf.find_containing_triangle_fast(&grid, 1.5, 0.2),
            surf.find_containing_triangle(1.5, 0.2)
        );
        assert_eq!(
            surf.find_containing_triangle_fast(&grid, 0.5, 0.2),
            surf.find_containing_triangle(0.5, 0.2)
        );
    }
}
