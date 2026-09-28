# Milestones 3 & 4 — Surface Boundaries, Bounded Volumes, General Contours, LandXML 1.2 Export

Technical paper describing the implementation in the `landsurvey` engine crate
(`crates/landsurvey/src/`) and its command wiring (`src/dispatch.rs`).
Verified with `cargo test -p landsurvey`: **38 lib + 5 golden + 1 integration, 0 failed**,
and zero warnings in the modified modules (`surface.rs`, `landxml.rs`).

Contents:

1. Starting point and scope
2. Data model (`Surface`, `ContourLine`)
3. Milestone 3 — boundaries and bounded volumes
4. Restored volume core (exact datum / overlay / grid)
5. The `plane_z` bug that broke the terrain-vs-terrain golden
6. Milestone 4a — general contours
7. Milestone 4b — LandXML 1.2 import + export
8. Command wiring (`src/dispatch.rs`)
9. Tests and acceptance
10. Known non-goals / pre-existing issues

---

## 1. Starting point and scope

Milestones 1 & 2 (CDT vertex insertion, UI wiring) were already functional.
The working tree contained a **broken partial attempt** at Milestones 3 & 4:

- `surface.rs` had duplicate method definitions (`apply_outer_boundary`,
  `add_hide_boundary`, `rebuild`, `generate_contours`,
  `contour_segments_at_elevation`), a `struct ContourLine` illegally nested
  inside `impl Surface`, and references to deleted functions (`delaunay`,
  `tri_cut_fill_seg`, `breakline_bbox`).
- `landxml.rs` contained only an exporter with two broken `writeln!` format
  strings and a wrong `xml_escape` (`&` mapped to `&`), while the importer
  (`looks_like_landxml`, `read_surfaces`, `read_first_surface`) used by
  `dispatch.rs`, the CLI, and the golden tests was missing entirely.

The fix was a full rewrite of `surface.rs` (Delaunay + volumes + boundaries +
contours in one coherent file) and `landxml.rs` (importer + corrected
exporter), a one-line repair of the golden test's `Surface` literal, and
targeted `dispatch.rs` wiring. No changes to Milestone 1 & 2 behavior.

---

## 2. Data model

`crates/landsurvey/src/surface.rs:78-105`:

```rust
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
```

Design notes:

- All fields stay `pub` for backward compatibility; the two new fields use
  `#[serde(default)]` so old serialized surfaces still deserialize.
- `Default` is implemented explicitly (empty name, no triangles, no
  boundaries), which also fixes struct literals such as the golden test's flat
  plane, now written as:

```rust
let flat = Surface {
    nodes: vec![[e0, n0, 1190.0], [e1, n0, 1190.0], ...],
    triangles: vec![[0, 1, 2], [0, 2, 3]],
    ..Default::default()
};
```

- `ContourLine` is a top-level type (`surface.rs:70-76`):

```rust
pub struct ContourLine {
    pub elevation: f64,
    pub is_major: bool,
    pub points: Vec<[f64; 3]>,
}
```

---

## 3. Milestone 3 — boundaries and bounded volumes

### 3.1 Predicate: strictly-inside point in polygon

`Surface::point_in_polygon` is a ray-casting test with two deliberate choices:

1. A duplicated closing vertex is ignored for the edge walk.
2. A point lying **on** any boundary edge returns `false` ("strictly inside"),
   via an explicit `point_on_segment` check before casting the ray.

Triangle membership is tested at the 2-D centroid
`((x1+x2+x3)/3, (y1+y2+y3)/3)`:

```rust
fn triangle_centroid_in_polygon(tri: &Tri, nodes: &[Node], poly: &[[f64; 2]]) -> bool {
    let a = nodes[tri[0]];
    let b = nodes[tri[1]];
    let c = nodes[tri[2]];
    let centroid = [(a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0];
    Self::point_in_polygon(centroid, poly)
}
```

### 3.2 Outer and hide boundaries

`surface.rs:334-359`:

```rust
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
```

Notes:

- Validation counts **distinct** vertices within 1e-9, so both open rings
  (`[A, B, C]`) and explicitly closed rings (`[A, B, C, A]`) are accepted, while
  degenerate inputs (`[[0,0],[1,1]]`) are rejected.
- Filtering is incremental over the **current** triangle list (per the spec),
  not a re-triangulation — breakline constraints and prior clips are preserved,
  and repeated calls compose (outer then hole, or several holes).
- The polygon is stored on the surface, so the operation is inspectable and
  the bounded surface can be re-exported.

### 3.3 Bounded volumes

Bounding is a preprocessing step, not a fourth volume method. `LS_VOLUME` with
`boundary=<polyline>` clones top and bottom, applies the same outer boundary
to **both**, then runs the unchanged exact-overlay and grid paths (Section 8).

---

## 4. Restored volume core

The rewrite restores the three earthwork paths the golden tests pin:

- `volume_to_datum` / `cut_fill_to_datum(_detailed)` — exact per-facet prism
  `area_xy · mean(z − datum)`; straddling triangles are split exactly by
  clipping the `(xy, h)` triangle against the `h ≥ 0` / `h ≤ 0` half-planes
  (`clip_polygon_by_height` + `polygon_height_volume`: fan triangulation,
  `Σ area · mean_h`). The crossing segment is also returned as the datum
  contour.
- `composite_cut_fill_detailed` / `exact_composite_cut_fill` — exact TIN
  overlay, documented `O(Nt·Nb)`: each top facet is clipped against each
  bottom facet (convex Sutherland–Hodgman, `clip_polygon`), `dz = top − bottom`
  is linear on every overlap cell, integrated exactly and split at `dz = 0`;
  zero-crossings accumulate into `cutfill_line`.
- `grid_cut_fill` — column method over the overlap extent at cell centres;
  cells where either surface is undefined are skipped.

Supporting geometry was also restored in-file: Bowyer–Watson `delaunay` (super-
triangle, `in_circumcircle` with 1e-12 threshold, CCW enforcement, exact-
duplicate skip), `orient2d`, `clip_polygon` / `inside_half_plane` /
`line_intersection`, and `breakline_bbox` for `find_affected_triangles`.

---

## 5. The `plane_z` bug that broke the terrain-vs-terrain golden

After restoring the overlay, 4 of 5 golden tests passed. The remaining failure:

```text
terrain_vs_terrain_matches_civil3d: cut 595307.67 != C3D Fill golden 590737.45
```

Diagnosis: the compressed fixture rounds every XY coordinate to 4 decimals, so
all 1770 points differ slightly from the road surface (verified with a script:
`xy diff count = 1770`, faces identical). Overlap vertices therefore sit on
both facet planes only **up to rounding noise**. The first version evaluated
`dz` with a containment query:

```rust
let zt = interp_on_triangle(&tp, p[0], p[1]).unwrap_or(0.0);
```

Any vertex missed by the `-1e-9` barycentric tolerance collapsed its elevation
to `0.0` (≈1190 m of spurious `dz`), biasing cut by +0.77% and halving the
zero-line count (297 vs ~363 segments).

Fix (`surface.rs:1060-1070`): evaluate the supporting **plane** unconditionally
— correct on the facet and exact at overlap vertices up to noise:

```rust
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
```

A second sign bug was fixed at the same time: `polygon_height_volume` returns a
magnitude (`|∫h|`), so the below-plane contribution must be **added**, not
negated (`fill += volume(below)`, not `fill += -volume(below)`). After both
fixes all 5 golden tests pass, including the flat-plane overlay consistency
check against the datum path.

---

## 6. Milestone 4a — general contours

`Surface::generate_contours(interval, major_every, smooth)` (`surface.rs:369+`):

1. `Zmin`/`Zmax` from nodes; level indices `k_start = ceil(Zmin/interval)`,
   `k_end = floor(Zmax/interval)`; each level is exactly `Zk = k · interval`.
2. `is_major = k.abs() % max(major_every,1) == 0` — correct for negative
   elevations.
3. Per level, every triangle is intersected with the plane `Z = Zk`
   (`contour_segments_at_elevation`): proper edge crossings are interpolated;
   single-vertex touches are collected from the incident edges and deduplicated
   (1e-9); coplanar triangles are skipped; a triangle yields 0 or 1 segment.
4. Segments are stitched into polylines (`stitch_segments`) with an adjacency
   map keyed by 2-D coordinates rounded to 1 micron (`(x·1e6).round()`), walking
   forward from the tail then backward from the head.
5. Optional Chaikin corner-cutting, 2 iterations (`chaikin_smooth`): closed
   loops (first ≈ last within 1e-9) wrap around; open polylines preserve both
   endpoints. Elevations are re-pinned to exactly `Zk` afterward.

---

## 7. Milestone 4b — LandXML 1.2 import + export

`crates/landsurvey/src/landxml.rs` implements both directions with `std` only.

**Import** (restored): `looks_like_landxml` (`<LandXML` heuristic),
`read_surfaces` (scans `<Surface>` blocks, skipping the `<Surfaces>`
container), `read_first_surface`, and a `parse_surface` alias. Point parsing
handles both encodings:

- Content form `<P id="0">N E Z</P>` → node `[E, N, Z]` (world `X = Easting`,
  `Y = Northing`, per OCS issue #157).
- Attribute form `<P id x y z/>` (our own export) → node `[x, y, z]`.

Faces `<F>i1 i2 i3</F>` resolve through an id→index map (0- and 1-based ids both
work); invisible `<F i="1">` faces, dangling references, and degenerate
sections yield no surface rather than corrupt geometry.

**Export** (`export_surface_to_landxml`, `landxml.rs:271-322`): valid LandXML
1.2 with `<Metric areaUnit="squareMeter" linearUnit="meter"
volumeUnit="cubicMeter" …/>`, `<Project>`, `<Surface name>`,
`<Definition surfType="TIN">`, 1-based `<P id x y z/>` points and 1-based
`<F>` faces. The pre-existing bugs (misplaced quotes in two `writeln!`
format strings, `xml_escape` emitting bare `&`/`<`/`>`/`"`) are fixed:

```rust
fn xml_escape(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&apos;"),
            _ => result.push(ch),
        }
    }
    result
}
```

Round-trip (`landxml::tests::landxml_round_trip_preserves_topology`) exports a
5-point surface named `TEST & <SURFACE>`, asserts escaping, re-imports, and
checks node/face counts, name recovery, and coordinates within 1e-6.

---

## 8. Command wiring (`src/dispatch.rs`)

- `LS_VOLUME … [boundary=<polyline>]`: token parsing accepts `boundary=` in any
  case; the referenced polyline is resolved by `find_boundary_polygon` (entity
  handle first, then case-insensitive `LwPolyline` layer match); both surfaces
  are cloned, `apply_outer_boundary` is applied to each, and the standard
  exact-overlay (+ optional grid) reporting and `draw` path run on the bounded
  copies.
- `LS_SURFACE_BOUNDARY <surface> <polyline> [outer|hide]`: resolves the surface
  through the same session → tagged-geometry → file chain as `LS_VOLUME`,
  closes the ring if needed, applies the boundary, updates the session cache,
  then `push_undo` + `draw_tin` on `LS-TIN-<NAME>` + `bump_geometry`/`set_dirty`
  with a triangle/edge count report.
- `LS_CONTOUR <surface> [interval] [major_every] [smooth]`: accepts positional
  **or** `key=value` tokens (`interval=`, `major_every=`/`major=`,
  `smooth=true/1/yes/on`); renders to `LS-CONTOUR-MAJOR` / `LS-CONTOUR-MINOR`
  `LwPolyline`s and reports level count and parameters.
- `LS_LANDXMLEXPORT <surface> <output_path>`: the path is the remainder of the
  line after the surface name (spaces allowed); the surface is resolved by name
  and written with a pts/faces summary.

Shared helper:

```rust
fn find_boundary_polygon(host: &mut dyn HostApi, poly_ref: &str) -> Option<Vec<[f64; 2]>> {
    // By handle, then by LwPolyline layer name (case-insensitive).
}
```

---

## 9. Tests and acceptance

- New unit tests in `surface.rs`: 100×100 flat grid clipped to a 10–90 window
  (asserts every surviving centroid is strictly inside and the count shrinks);
  a 40–60 hide hole (asserts no surviving centroid lies inside); a pyramid
  contoured at interval 1.0 / major 5 (exact `Z` on every point, `is_major`
  from `k`, smoothing preserves line count).
- New tests in `landxml.rs`: export→import round-trip (counts, name with
  XML metacharacters, coordinates) and content-form `N E → X E / Y N` mapping
  with dangling-face rejection.
- Golden suite unchanged in expectations: topology/area, below-surface datum,
  crossing datum, flat-plane overlay consistency, terrain-vs-terrain — all pass
  against the Civil 3D 2026 values.
- Final result: `cargo test -p landsurvey` → **38 + 5 + 1 passed, 0 failed**;
  no warnings reference `surface.rs` or `landxml.rs` (the 15 remaining warnings
  are pre-existing `featureline` unused-import/variable notices).

---

## 10. Known non-goals / pre-existing issues

- `cargo check` on the `opencad-landsurvey-plugin` cdylib reports host-API drift
  errors (`entities().get`, `LwPolyline::vertices`, `CommandStep::Finish`,
  `InteractiveCommand::on_finish`, …) that pre-date this work and affect the
  old code paths identically; Milestone 3 & 4 acceptance is scoped to
  `cargo test -p landsurvey` per the task and the OOM memory note.
- Volumes remain `O(Nt·Nb)` overlay and `O(n²)` Bowyer–Watson with linear point
  queries — documented in-code, unchanged by this milestone.
- Centroid filtering (not exact polygon clipping) defines boundary membership;
  triangles straddling a boundary are kept or dropped whole by centroid.
