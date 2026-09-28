# Grading Engine — Phase 1 Technical Report

| | |
|---|---|
| **Status** | Complete — all acceptance gates green |
| **Date** | 2026-09-26 |
| **Crate** | `crates/landsurvey` (`opencad_landsurvey` workspace member) |
| **Toolchain** | Rust `1.98.1` (pinned by `rust-toolchain.toml`) |
| **Scope** | `crates/landsurvey/src/grading/{mod,error,criteria,math}.rs` |
| **Test total** | **78 passed / 0 failed** (baseline 66 → +12) |

---

## 1. Executive summary

Phase 1 delivered the foundation layer of the grading engine: a structured
error taxonomy, a validated grading-criteria model, and a numerically hardened
geometry kernel for bulge-arc densification, outward normals, corner bisectors
and convexity tests.

All four target files were written from scratch (the module previously existed
only as `pub mod grading;` in `lib.rs` with no children), the module is
`std`-only with **zero new dependencies**, contains no `unsafe`, and every
routine is a pure, deterministic function of its inputs. Verification:

| Gate | Command | Result |
|---|---|---|
| Compile (all targets) | `cargo check --workspace --all-targets` | 0 errors / 0 warnings |
| Formatting | `cargo fmt --all -- --check` | exit 0 |
| Tests | `cargo test --workspace --locked` | **78 passed, 0 failed** (target ≥ 72) |
| Rustdoc | `cargo doc --workspace --no-deps` | `landsurvey`: 0 warnings |
| Lockfile | `--locked` accepted | `Cargo.lock` byte-identical (no deps added) |
| Clippy | — | not run (component not installed; CI does not run it) |

---

## 2. Scope

**In scope**

1. `grading/error.rs` — `GradingError` enum with `Display` + `std::error::Error`.
2. `grading/criteria.rs` — `SlopeValue`, `GradingTarget`, `GradingCriteria` with
   `Default`, `validate()`, `Display`.
3. `grading/math.rs` — full rewrite of the bulge/normal/bisector kernel to the
   specified `Result`-based contract.
4. `grading/mod.rs` — submodule declarations and public re-exports.
5. Acceptance: check, fmt, test ≥ 72, CI (`--locked`) compatibility.

**Out of scope (Phase 2+)**

- Daylight / projection ray casting (surface, elevation, distance, point targets).
- Feature-line stationing on `sampling_interval`, corner offsetting at scale.
- DXF / LandXML / LandXML output of grading results.
- `cargo clippy` (binary not installed for the pinned toolchain; absent from CI).

No file outside `crates/landsurvey/src/grading/` was modified; `lib.rs` already
declared `pub mod grading;`.

---

## 3. Deliverables

| File | Lines | Contents |
|---|---:|---|
| `grading/error.rs` | 109 | `GradingError` (6 variants), `Display`, `std::error::Error`, 2 tests |
| `grading/criteria.rs` | 243 | `SlopeValue`, `GradingTarget`, `GradingCriteria`, validation, 4 tests |
| `grading/math.rs` | 511 | densification + normal/bisector/orientation kernel, 10 tests |
| `grading/mod.rs` | 21 | `pub mod error/criteria/math` + curated re-exports |
| **Total** | **884** | **16 tests** |

### Public API

```rust
// mod.rs re-exports
pub use criteria::{GradingCriteria, GradingTarget, SlopeValue};
pub use error::GradingError;
pub use math::{densify_bulge, is_convex_corner, orient2d,
               outward_normal_2d, vertex_bisector_2d};

// math.rs signatures
pub fn densify_bulge(p1: [f64; 3], p2: [f64; 3], bulge: f64, max_sagitta: f64)
    -> Result<Vec<[f64; 3]>, GradingError>;
pub fn outward_normal_2d(p1: [f64; 2], p2: [f64; 2])
    -> Result<[f64; 2], GradingError>;
pub fn vertex_bisector_2d(prev: [f64; 2], curr: [f64; 2], next: [f64; 2])
    -> Result<[f64; 2], GradingError>;
pub fn orient2d(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64;
pub fn is_convex_corner(prev: [f64; 2], curr: [f64; 2], next: [f64; 2]) -> bool;
pub fn left_normal(p1: [f64; 2], p2: [f64; 2]) -> Option<[f64; 2]>;
pub fn right_normal(p1: [f64; 2], p2: [f64; 2]) -> Option<[f64; 2]>;
pub fn bisector_at(prev: [f64; 2], curr: [f64; 2], next: [f64; 2]) -> Option<[f64; 2]>;
```

Private helpers: `arc_segment_count`, `is_finite2`, `is_finite3`
(plus test helper `max_sagitta_of`).

---

## 4. Module architecture

```
grading/
├── mod.rs      → declares children, re-exports the consumer-facing surface
├── error.rs    → GradingError                    (leaf: no internal deps)
├── criteria.rs → SlopeValue, GradingTarget,
│                 GradingCriteria                 (depends on error)
└── math.rs     → densification / normals /
                  bisectors / orientation         (depends on error)
```

Layering is acyclic: `error` ← `criteria`/`math`. `criteria` never touches
`math`; geometry routines never touch `criteria`. Both converge on the single
error type, so callers match on one enum regardless of which layer failed.

---

## 5. Error model

### 5.1 `GradingError`

| Variant | Meaning | Constructed by (Phase 1) |
|---|---|---|
| `DegenerateSegment` | segment shorter than the degeneracy tolerance — no direction/arc derivable | `math::{densify_bulge, outward_normal_2d}` |
| `InvalidCriteria(String)` | criteria field out of range / non-finite; payload names the field | `GradingCriteria::validate` |
| `CalculationOverflow` | an input or derived intermediate is non-finite (NaN/±inf) | `math::densify_bulge`, `math::outward_normal_2d` |
| `CollinearPoints` | collinear vertices where a corner was required | *reserved — Phase 2 (miter/offset joints)* |
| `RayTargetNotFound` | daylight ray never hit its target in the search window | *reserved — Phase 2* |
| `RayLimitExceeded` | ray exceeded `max_projection_distance` | *reserved — Phase 2* |

`Display` renders stable, non-empty sentences (asserted by test);
`impl std::error::Error` enables `?` into `Box<dyn Error>`.
Variants are deliberately coarse: they classify *why* a calculation failed,
since the caller already holds the geometric context.

### 5.2 `densify_bulge` check order (contract)

1. Non-finite `p1` / `p2` / `bulge` → `Err(CalculationOverflow)`.
2. `chord < DEGENERATE_TOL` → `Err(DegenerateSegment)` — **checked before the
   straight-segment shortcut**, so a zero-length segment is rejected whatever
   its bulge is.
3. `|bulge| < STRAIGHT_BULGE_TOL` → `Ok(vec![p1, p2])` (two points, bit-exact).
4. Non-finite derived radius / center offset / center → `Err(CalculationOverflow)`.
5. Otherwise densify to `n_segs + 1` points and pin both endpoints.

### 5.3 `GradingCriteria::validate` rules

| Field | Rule | Error text contains |
|---|---|---|
| `sampling_interval` | finite and `> 0` | `sampling_interval` |
| `max_sagitta` | finite (**`≤ 0` allowed** → selects the fixed-step fallback) | `max_sagitta` |
| `max_projection_distance` | finite and `> 0` | `max_projection_distance` |
| `cut_slope` / `fill_slope` | every component finite | `cut_slope` / `fill_slope` |

First offending field wins (deterministic order: interval → sagitta →
projection → cut → fill).

### 5.4 Slope normalization

`SlopeValue::as_gradient() -> f64` returns the dimensionless **run/rise** value
used by daylight equations:

* `Ratio { h, v }` → `h / v`, or `0.0` when `|h| < 1e-9` (guard also turns the
  indeterminate `0/0` into `0.0` rather than NaN).
* `Percent(p)` → `p / 100.0` (stored as the percentage itself, `25.0`, not `0.25`).

Defaults: `target = Surface`, `cut = 2:1`, `fill = 3:1`, `sampling_interval = 1.0 m`,
`max_sagitta = 0.01 m`, `max_projection_distance = 500.0 m`.

---

## 6. Mathematical foundations

### 6.1 Bulge geometry (`densify_bulge`)

Definitions, for a chord vector `d = p2 − p1`, `c = |d|`, bulge `b`:

| Quantity | Formula |
|---|---|
| sweep angle | `θ = 4 · atan(b)` (sign = traversal direction) |
| radius | `R = c · (1 + b²) / (4 · |b|)` (always positive) |
| midpoint → center offset | `δ = c · (1 − b²) / (4 · |b|)` — **signed**; negative for `|b| > 1` (major arcs), zero at `|b| = 1` (semicircle) |
| center | `C = midpoint + side · n_left · δ`, `side = +1` if `θ > 0` else `−1`, `n_left = (−dy, dx)/c` |

The signed offset correctly flips the center to the far side of the chord for
arcs greater than 180°, and the `side` factor places a CCW sweep (`b > 0`)
with its center to the left of the chord. Samples are generated by polar
integration from `start_angle = atan2(p1 − C)` with `step = θ / n_segs`.

**Endpoint pinning:** index `0` is overwritten with `p1` and `n_segs` with `p2`
bit-exactly, because angle integration drifts ~1e-12 over long sweeps.

**Elevation:** `z_i = lerp(p1.z, p2.z, i / n_segs)` — linear in normalized arc
length (uniform in sweep angle), hence monotonic whenever `p1.z ≠ p2.z`
(asserted by test).

### 6.2 Sagitta bound → segment count (`arc_segment_count`)

Chord-to-arc sagitta for a half-step `φ`:

```
s = R · (1 − cos(φ/2))   ⇒   φ_max = 2 · acos(1 − s/R)
n = ceil(|θ| / φ_max)
```

with these guards (all inside the helper, `n` returned):

| Condition | Result |
|---|---|
| `s` non-finite or `s ≤ 0`, or `R` invalid | fixed fallback step `FALLBACK_STEP = π/8` (22.5°) |
| bound collapses (`acos` resolution, `φ_max ≤ 0`, non-finite) | saturate at `MAX_SEGMENTS` |
| always | `ceil().max(2.0).min(MAX_SEGMENTS as f64) as usize` |

Because `f64::{max,min}` ignore NaN and `inf.min(x) == x`, **no input can escape
the `2..=1_000_000` window** — `inf as usize` (which would be `usize::MAX` and
OOM) is unreachable. `ceil()` guarantees whole segments; `max(2.0)` guarantees
the `≥ 3 points` output floor required by the contract.

### 6.3 Outward normal and bisectors

* **Convention:** grading boundaries wind **clockwise**; the outward side of a
  segment is its **right-hand** side.
  `outward_normal_2d(p1, p2) = (dy, −dx) / ‖d‖` — eastward segments yield
  `[0, −1]`, northward `[1, 0]`. Errors: `DegenerateSegment` (`‖d‖ < 1e-9`),
  `CalculationOverflow` (non-finite input).
* **`vertex_bisector_2d`** (exterior/outward bisector): `normalize(N1 + N2)`.
  On a straight run it coincides with the segment normal; at a genuine corner it
  bisects the exterior angle. A 180° turnaround cancels the normals
  (`‖N1+N2‖ < BISECTOR_TOL`) → falls back to `N1`, so the result stays finite
  and unit-length instead of NaN.
* **`bisector_at`** (interior/miter direction, legacy): normalized sum of the
  incoming/outgoing **unit tangents**; returns `None` on a degenerate leg or an
  exact U-turn (where the miter is undefined).
* **`left_normal` / `right_normal`**: `Option`-based wrappers over
  `outward_normal_2d` (`right = outward`, `left = −outward`) kept for existing
  callers.

The two bisector families are intentionally different objects: outward-offset
corners use `vertex_bisector_2d`; tangent-sum miters use `bisector_at`.

### 6.4 Orientation / convexity

```
orient2d(a, b, c) = (b − a) × (c − a)          // = 2·signed area of △abc
is_convex_corner  = orient2d.is_finite() && orient2d < 0.0
```

Positive ⇒ counter-clockwise turn; negative ⇒ clockwise turn. For a
clockwise-wound boundary a **right turn** is a convex corner, hence the `< 0`
test. Collinear vertices (`o == 0`) are not corners, and a non-finite /
overflowing result is treated as *unclassifiable → not a corner* rather than
guessed.

---

## 7. Design decisions and rationale

| # | Decision | Rationale |
|---|---|---|
| 1 | Degeneracy check **before** the straight shortcut in `densify_bulge` | `Ok([p1, p2])` for a zero-length chord would silently fabricate a segment with no direction; the contract rejects it instead. |
| 2 | Unusable `max_sagitta` ⇒ fixed-step fallback, **not** an error | Mirrors the explicit contract; criteria validation separately guarantees a *finite* sagitta, so only a deliberate `≤ 0` reaches the fallback. |
| 3 | `MAX_SEGMENTS = 1_000_000` saturation | Bounds work and memory (≤ ~24 MiB for one call: 1,000,001 × 24 B); a 1:10⁶ tolerance would otherwise request astronomically many segments. Trade-off: the sagitta guarantee is relaxed at the cap (documented). |
| 4 | Single tolerances `1e-9` (`DEGENERATE_TOL`, `STRAIGHT_BULGE_TOL`, `BISECTOR_TOL`) | One documented machine-epsilon-scale threshold family; legacy `1e-12` degeneracy threshold in the old normal helpers was unified to `1e-9` — a behavior change in the retained `Option`-based API, noted below. |
| 5 | Endpoint pinning (`pts[0] = p1`, `pts[n] = p2`) | Removes integration drift so callers can concatenate densified runs without gaps. |
| 6 | Right normal = outward | Matches the clockwise winding convention used by grading boundaries; a single convention avoids sign bugs in Phase 2 offsets. |
| 7 | U-turn fallback to `N1` (`vertex_bisector_2d`) vs `None` (`bisector_at`) | Outward offsetting must always produce a usable finite direction; the miter API historically signals "undefined" and preserves that. |
| 8 | Coarse error enum, one type per layer boundary | Callers match causes structurally instead of parsing strings; `String` payload only for criteria field names. |
| 9 | Reserved-but-unconstructed variants shipped now | Locks the public error surface before Phase 2 consumes it, avoiding a later breaking change. |

**API behavior change (intentional):** `left_normal`, `right_normal`,
`bisector_at` previously used a `1e-12` degeneracy threshold; they now share
`DEGENERATE_TOL = 1e-9` / `BISECTOR_TOL = 1e-9`. Segments between 1e-12 and
1e-9 long previously returned a unit normal and now return `None`.

---

## 8. Safety, determinism, performance

* **Zero `unsafe`**, zero dependencies beyond pre-existing
  (`serde`, `serde_json`, `chrono`); `std`/`core` only.
* **100% deterministic:** no RNG, no clock, no hash-map iteration, no global
  state — identical inputs yield bit-identical output across runs and platforms.
* **No NaN/inf leakage:** every entry point validates finiteness before doing
  arithmetic; every division is preceded by a magnitude check; every `as`-cast of
  a float goes through a clamped range first.
* **Bounded allocation:** `Vec::with_capacity(n_segs + 1)` with
  `n_segs ≤ 1_000_000` ⇒ worst case ≈ 24 MiB for one arc; the common case
  (1 m chord, 1 cm sagitta on a 10 m radius arc) allocates tens of points.
* **Pure functions:** all routines are side-effect free and trivially unit
  testable without fixtures.

---

## 9. Verification

### 9.1 Gates

| Gate | Command | Result |
|---|---|---|
| Compile | `cargo check --workspace --all-targets` | clean — 0 errors, 0 warnings |
| Format | `cargo fmt --all -- --check` | exit 0 (no `.rustfmt.toml` → default style) |
| Tests | `cargo test --workspace --locked` | **78 passed / 0 failed / 0 ignored** |
| Rustdoc | `cargo doc --workspace --no-deps` | `landsurvey`: 0 warnings |

Rustdoc note: three intra-doc links to private constants were found and fixed
during this phase (links replaced with code spans). The `opencad-landsurvey-plugin`
crate still reports 2 pre-existing rustdoc warnings (links to its private
`ribbon` / `dispatch` modules) — unrelated to grading and out of scope.
`cargo clippy` could not be run: `cargo-clippy.exe` is not installed for the
`1.98.1-x86_64-pc-windows-msvc` toolchain, and CI does not invoke clippy.

### 9.2 Test distribution

| Test binary | Tests |
|---|---:|
| `landsurvey` (lib) | 62 *(46 pre-existing + **16 grading**)* |
| `tests/road_surface_volume_golden.rs` | 5 |
| `tests/volume_pnezd.rs` | 1 |
| `landsurvey-cli` | 1 |
| `opencad-landsurvey-plugin` (lib) | 9 |
| rustdoc tests | 0 |
| **Total** | **78** |

Baseline before Phase 1 was 66 (the old `math.rs` held 4 tests); Phase 1 adds
+12 net (math 4 → 10, criteria +4, error +2).

### 9.3 Grading test inventory (16)

**`math.rs` (10)**

| Test | Asserts |
|---|---|
| `straight_bulge_returns_endpoints` | `|b| < 1e-9` ⇒ exactly `vec![p1, p2]` |
| `densify_rejects_degenerate_and_non_finite` | zero chord ⇒ `DegenerateSegment`; NaN/inf ⇒ `CalculationOverflow`; ordering of the checks |
| `quarter_arc_respects_sagitta_and_elevation` | every sample within `max_sagitta` of the true circle; endpoints bit-exact; z interpolated |
| `quarter_arc_elevation_is_monotonic` | z strictly increasing along the densified arc |
| `semicircle_and_direction_signs` | sweep sign / bulge sign ⇒ correct side; `|b| = 1` center at chord midpoint |
| `arc_segment_count_stays_within_bounds` | result always in `2..=1_000_000`, including collapsed/non-finite tolerances |
| `outward_normal_2d_is_unit_right_and_error_free` | unit length, right-of-direction sign, east/north cases, error paths |
| `vertex_bisector_2d_corner_straight_and_turnaround` | straight run ⇒ segment normal; 90° corner ⇒ 45° bisector; U-turn ⇒ finite fallback (no NaN) |
| `is_convex_corner_follows_turn_direction` | right turn ⇒ `true` (CW winding), left turn ⇒ `false`, collinear ⇒ `false` |
| `normals_and_bisector` | `left/right_normal` = `∓outward`; `bisector_at` miter direction + `None` on U-turn/degenerate |

**`criteria.rs` (4)** — `default_criteria_values`, `slope_value_gradients`
(`2:1 → 2.0`, `0:1 → 0.0`, `25% → 0.25`), `validate_rejects_bad_fields`
(four error paths, message field names), `display_mentions_every_field`.

**`error.rs` (2)** — `display_is_stable_and_non_empty` (all 6 variants),
`behaves_as_std_error` (boxable as `dyn Error`, `source()` safe).

---

## 10. CI and lockfile compatibility

`.github/workflows/ci.yml` runs:

* `cargo metadata --locked`
* `cargo build --release --locked`
* `cargo test --workspace --locked`

Phase 1 added **no dependencies and touched no manifest**, so `Cargo.lock` is
byte-identical; `cargo test --workspace --locked` passing locally is direct
proof the lockfile gate will hold. There is no clippy job in CI, so the missing
clippy component does not affect the pipeline.

---

## 11. Known limitations and Phase 2 follow-ups

1. **Reserved errors are not yet constructed.** `CollinearPoints`,
   `RayTargetNotFound`, `RayLimitExceeded` exist and are covered by
   `Display` tests only — Phase 2 (daylight projection) will start producing them.
2. **`Ratio { h ≠ 0, v = 0 }` passes `validate()` but yields `±inf`** from
   `as_gradient()`. Validation intentionally checks *finiteness* only (per the
   Phase 1 contract); a vertical face should be rejected or special-cased when
   the daylight equation lands in Phase 2.
3. **`orient2d` uses a plain f64 cross product** — no adaptive/robust predicates
   (Shewchuk-style). At survey coordinate magnitudes the relative error is
   negligible; the guard is that a non-finite result is classified as
   "not a corner", never a wrong `true`.
4. **`MAX_SEGMENTS` relaxes the sagitta bound** rather than failing; only
   reachable with extreme tolerance/radius ratios.
5. **Interior arc points carry residual radial error** (~1e-12–1e-9 relative);
   endpoints are exact. Tests bound it at `1e-9` absolute for unit-scale arcs.
6. **Clippy not executed** (component absent from the pinned toolchain; not in CI).
7. **Legacy threshold unification** (`1e-12 → 1e-9`) in
   `left_normal`/`right_normal`/`bisector_at` is a silent behavior change for
   micro-segments — flagged here for audit.

---

## Appendix A — Module constants

| Constant | Value | Role |
|---|---|---|
| `STRAIGHT_BULGE_TOL` | `1e-9` | `|bulge|` below ⇒ straight 2-point segment |
| `DEGENERATE_TOL` | `1e-9` | chord / segment length below ⇒ `DegenerateSegment` |
| `BISECTOR_TOL` | `1e-9` | `‖N1+N2‖` (or tangent sum) below ⇒ U-turn fallback / `None` |
| `FALLBACK_STEP` | `π / 8` (22.5°) | fixed angular step when `max_sagitta` is non-positive/non-finite |
| `MAX_SEGMENTS` | `1_000_000` | hard cap on arc segment count (≈ 24 MiB worst case) |

All constants are private to `math.rs`; consumers pass tolerances as arguments
(`max_sagitta`) and read semantics from the docs.

## Appendix B — Command transcript (acceptance run)

```
cargo check --workspace --all-targets   → 0 errors, 0 warnings (exit 0)
cargo fmt --all -- --check              → exit 0
cargo test --workspace --locked         → 78 passed, 0 failed (exit 0)
cargo doc --workspace --no-deps         → landsurvey: 0 warnings
```

Workspace test breakdown: `62 + 5 + 1 + 1 + 9 = 78`.
