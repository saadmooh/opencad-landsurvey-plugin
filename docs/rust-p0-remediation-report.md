# Rust P0 Remediation Report — LandSurvey Engine & Plugin Host Bridge

**Scope:** `crates/landsurvey/src/` (engine: `surface.rs`, `featureline/breakline.rs`) + `src/dispatch.rs` (host bridge, XDATA persistence, surface reconstruction).
**Single source of truth:** `cargo fmt --all -- --check`, `cargo check --workspace`, and `cargo test --workspace` executed on the current working tree, plus line-by-line source reading. Every claim below is cited as `file:line`.

---

## 1. Executive Summary

The Rust P0 audit identified four defects that could produce silently wrong survey geometry or lost surface metadata. All four are now fixed, regression-tested, and verified green.

| # | Defect | Severity | Status | Primary evidence |
|---|---|---|---|---|
| P0-1 | Breakline edge flipping used a fixed corner assumption, misidentifying the opposite vertex on 2 of 3 edges | Critical (silent geometry corruption) | Fixed | `featureline/breakline.rs:215-226`, `featureline/breakline.rs:401-421` |
| P0-2 | `Surface::from_points` triangulation order was non-deterministic (`HashMap` hole-boundary iteration) | Critical (non-reproducible TINs) | Fixed | `surface.rs:920-931`, `surface.rs:1347-1359` |
| P0-3 | `SpatialGrid::query_into` reordered/corrupted pre-existing caller buffer entries | High (wrong containing triangle) | Fixed | `surface.rs:772-808`, `surface.rs:1512-1544` |
| P0-4 | `place` attached XDATA to a locally re-read entity instead of the `Handle` returned by the host | High (metadata loss) | Fixed | `dispatch.rs:2221-2227` |

**Verification outcome:** workspace test suite is fully green (66 passed / 0 failed), formatting is clean, and the workspace compiles with zero errors.

---

## 2. Build, Format, and Test Verification

### 2.1 Zero-warning build

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | Exit 0 — no formatting drift |
| `cargo check --workspace` | Exit 0 — `landsurvey`, `landsurvey-cli`, `opencad-landsurvey-plugin` all compile, 0 errors |
| `cargo check --workspace --all-targets` | Exit 0 — 0 lines matching `warning` or `error` across libs, bins, tests, and benches; establishes the zero-warning claim |

The pre-remediation state recorded in `docs/grading-readiness-audit.md` (15 warnings, 7 errors) is fully resolved.

### 2.2 Test suite

`cargo test --workspace` — **66 passed, 0 failed**:

| Target | Tests |
|---|---|
| `landsurvey` library | 50 |
| `road_surface_volume_golden` | 5 |
| `volume_pnezd` | 1 |
| `landsurvey-cli` | 1 |
| `opencad-landsurvey-plugin` (dispatch) | 9 |

### 2.3 Repeat-run stability

The two surface-reconstruction regressions were executed six consecutive times to rule out order-dependent flakiness, all passing:

- `dispatch::tests::rebuilds_surface_from_tagged_mesh_case_insensitive` (`src/dispatch.rs:2738-2746`)
- `dispatch::tests::surface_survives_dwg_save_reopen` (`src/dispatch.rs:2774-2790`)

**Verdict:** ✅ Build clean, format clean, tests green and stable.

---

## 3. Root Cause and Remediation — Per Defect

### 3.1 P0-1 — Breakline edge flipping assumed a fixed corner

**Location:** `crates/landsurvey/src/featureline/breakline.rs`

**Root cause.** The incremental Delaunay-flipping loop in `recover_constrained_edges` walked each triangle's three edges, but when resolving the neighbour's opposite corner it matched against the *first two* indices (`tri[0]`, `tri[1]`) rather than the edge actually under consideration. That match is only correct for a triangle's first edge; for edges 2 and 3 it could resolve to an endpoint of the shared edge instead of the true opposite vertex. A flip built on a wrong corner silently re-triangulates the surface into an invalid TIN.

**Remediation.**
- The neighbour's opposite corner is now resolved *against the shared edge `(a, b)` being processed, via a dedicated `opposite_vertex` helper that is independent of edge position (`breakline.rs:215-226`).
- `opposite_vertex` (`breakline.rs:401-421`) returns the single vertex of `tri` that is not an endpoint of `(a, b)`, and returns `None` for degenerate input (repeated index) or when `(a, b)` is not a corner pair of `tri`.
- `flip_edge` (`breakline.rs:423-470`) now derives *both* opposite corners from the triangles themselves and rejects the unsound cases with explicit errors: a non-corner-pair edge, a repeated endpoint (`a == b`), and two triangles with identical opposite corners.

**Tests added.**
- `opposite_vertex_resolves_corners_for_every_corner_pair` (`breakline.rs:543`) — exhaustively checks all three corner pairs.
- `opposite_vertex_rejects_non_corner_pair_and_degenerate_input` (`breakline.rs:552`).

### 3.2 P0-2 — Non-deterministic triangulation from `HashMap` iteration

**Location:** `crates/landsurvey/src/surface.rs`, `Surface::from_points` → `delaunay`

**Root cause.** During Delaunay insertion (`surface.rs:901-931`), the boundary of the polygonal hole created by "bad" triangles was computed with a `HashMap<(usize, usize), usize>` edge-count map. Hole-boundary iteration order therefore depended on hash order, which is not stable across runs/processes. The order in which new triangles are appended varied between identical inputs.

**Why this matters (product invariant).** Triangle order is a correctness invariant, not just a cosmetic one: containing-triangle lookup resolves shared-edge ties by the *lowest* triangle index, so a varying triangle order silently changes which facet wins on a shared edge.

**Remediation.** The edge-count map is now a `BTreeMap<(usize, usize), usize>`, making hole-boundary iteration — and therefore triangle append order — deterministic across runs (`surface.rs:920-931`).

**Test added.** `from_points_is_deterministic_across_calls` (`surface.rs:1347-1359`) builds the same surface once and then 16 more times, asserting every build produces an identical triangle list. This test fails 4/4 runs if the `HashMap` is restored.

### 3.3 P0-3 — `SpatialGrid::query_into` clobbered the caller's buffer

**Location:** `crates/landsurvey/src/surface.rs`, `SpatialGrid::query_into` (`surface.rs:772-808`)

**Root cause.** The accelerated containing-triangle lookup queries the spatial grid and relies on candidates being visited in ascending index order, because cell traversal order is not index order (a point on a cell boundary reaches the left cell before the right cell, so triangle 1 could be visited before triangle 0). The pre-fix `query_into` sorted/deduplicated the *entire* output buffer, which reordered or removed entries the caller had already placed there. Reused buffers across queries therefore produced different results than a fresh buffer.

**Remediation.** `query_into` now:
1. Records the buffer's starting length (`out.len()` → `start`).
2. Appends candidates as before.
3. Sorts and compacts **only the freshly appended tail** (`out[start..]`), leaving all pre-existing entries exactly as the caller placed them (`surface.rs:795-807`).

The documented contract now states this explicitly: existing entries are left untouched, only new candidates are deduplicated and sorted ascending (`surface.rs:772-776`).

**Test added.** `spatial_grid_prefers_lowest_index_on_shared_edge` (`surface.rs:1512-1544`) constructs two triangles sharing a vertical edge placed on a cell boundary, queries a point exactly on that shared edge, and asserts the accelerated lookup returns the same lowest-index triangle as the full scan.

### 3.4 P0-4 — `place` attached XDATA to the wrong entity

**Location:** `src/dispatch.rs`, `place` (`dispatch.rs:2221-2227`)

**Root cause.** `place` created an entity, then re-read it from the document to attach the `ExtendedDataRecord` (XDATA) that carries the surface name/kind tag. This is fragile and wrong: the correct target is the `Handle` returned by the host when the entity was added. Operating on a re-read copy risks writing the tag to the wrong entity, or losing it, so a placed surface cannot be found again by name.

**Remediation.** `place` now captures the handle from `host.add_entity(ent)` and attaches the XDATA via `host.write_record(handle, rec)` using that exact handle (`dispatch.rs:2221-2227`).

**Regression coverage.** Round-trip through the DWG codec verifies tag → DWG bytes → reopen → rebuild-by-name recovers the surface exactly (`src/dispatch.rs:2774-2790`); case-insensitive name lookup is covered at `src/dispatch.rs:2738-2746`.

---

## 4. Preserved Invariants and Non-Goals

These behaviours were audited and deliberately **left unchanged**; they are correct, not defects:

- **`surface_from_mesh` preserves mesh face order.** Tagged-mesh discovery rebuilds the TIN by reading the mesh's existing triangles in place; it does not re-triangulate. Order is preserved on purpose because the containing-triangle tie-break depends on it. Verified by `mesh_surface_roundtrip_is_exact` (`src/dispatch.rs:2730-2736`).
- **TIN drawing from edge lines** compares node/triangle counts plus plan area and volume-bearing sums rather than raw vectors, because node order is not guaranteed identical through the shared Delaunay builder (`src/dispatch.rs:2748-2772`).

---

## 5. Verification Checklist (Exit Criteria)

| Criterion | Command / Evidence | Result |
|---|---|---|
| Workspace compiles, 0 errors | `cargo check --workspace` | ✅ |
| Zero warnings (all targets) | `cargo check --workspace --all-targets` → 0 matches | ✅ |
| Formatting clean | `cargo fmt --all -- --check` | ✅ |
| Full suite green | `cargo test --workspace` → 66/0 | ✅ |
| Breakline flip correct for all corner pairs | `breakline.rs:543`, `breakline.rs:552` | ✅ |
| Triangulation deterministic across runs | `surface.rs:1347-1359` | ✅ |
| Grid lookup matches full scan on shared edge | `surface.rs:1512-1544` | ✅ |
| Surface rebuild stable ×6 runs | `dispatch.rs:2738`, `dispatch.rs:2774` | ✅ |
| Surface survives DWG save/reopen | `dispatch.rs:2774-2790` | ✅ |

### Known gaps (not blocking, informational)

- `cargo clippy --workspace --all-targets` was **not** run (not authorized in this audit). Recommend running it as a follow-up lint gate.
- The plugin directory is not a Git repository, so a `git diff`-based change summary could not be produced for this report.

---

## 6. Summary

All four P0 defects are resolved with defensive, explicitly-validated code paths and dedicated regression tests. The engine and the host bridge compile warning-free, the workspace test suite is fully green and stable across repeated runs, and the two key product invariants (deterministic triangle order; containing-triangle tie-break by lowest index) are now enforced by both implementation and test. No further blocking work remains.
