# Phase 1: Foundation - Technical Implementation Plan

**Duration**: 2-3 months  
**Target Parity**: 35% Civil 3D  
**Dependencies**: acadrust 3D polyline support (upstream)

---

## 1.1 True 3D Polyline Entity

### Objective
Replace 2D `LwPolyline` + XDATA CSV workaround with native 3D polyline entity supporting per-vertex Z.

### Technical Approach

#### Option A: Upstream acadrust 3D Polyline (Preferred)
```rust
// Target acadrust API extension
pub struct Polyline3d {
    pub vertices: Vec<Vector3>,  // X, Y, Z per vertex
    pub bulges: Vec<f64>,        // Per-segment bulge
    pub is_closed: bool,
    pub elevation: f64,          // Base elevation (for compatibility)
}
```

**Implementation Steps**:
1. Fork `acadrust` or submit PR to `HakanSeven12/cadcodec`
2. Add `Polyline3d` entity type with DXF/DWG support
3. Implement `EntityType::Polyline3d` variant
4. Add serialization/deserialization for DWG/DXF
5. Update `EntityCommon` to support 3D vertices

#### Option B: Custom Entity (Fallback)
If upstream delayed, implement as custom entity with XDATA:
```rust
// Store full 3D geometry in XDATA
struct FeatureLine3d {
    vertices: Vec<Point3d>,
    bulges: Vec<f64>,
    is_closed: bool,
    // Serialize to XDATA as binary or JSON
}
```

### Migration Path
1. Keep current `LwPolyline` + XDATA CSV as fallback
2. Detect acadrust version at runtime
3. Use `Polyline3d` when available, fallback otherwise
4. Automatic migration on load/save

### Acceptance Criteria
- [ ] Create 3D polyline with 10+ vertices, each with unique Z
- [ ] Save to DWG, reopen, verify Z preserved
- [ ] Export to DXF, verify Z in `VERTEX` entities
- [ ] Round-trip through OCS host without data loss

---

## 1.2 Interactive Point Picking

### Objective
Implement `InteractiveCommand` for real-time point picking on TIN surface.

### Architecture

```rust
// src/interactive/featureline_create.rs
use ocs_plugin_api::host::{HostApi, InteractiveCommand, InteractiveContext, PickResult};

pub struct FeatureLineCreateCommand {
    surface: Surface,
    surface_name: String,
    vertices: Vec<FeatureVertex>,
    state: CreateState,
    rubber_band: Option<RubberBand>,
}

enum CreateState {
    PickingFirstPoint,
    PickingNextPoint,
    ConfirmingZ { tin_z: f64, vertex_idx: usize },
    Finished,
}

impl InteractiveCommand for FeatureLineCreateCommand {
    fn on_pick(&mut self, ctx: &mut InteractiveContext, pt: Point3d) -> CommandResult {
        match self.state {
            CreateState::PickingFirstPoint => { /* ... */ }
            CreateState::PickingNextPoint => { /* ... */ }
            CreateState::ConfirmingZ { .. } => { /* ... */ }
            CreateState::Finished => CommandResult::Ignore,
        }
    }
    
    fn on_keyword(&mut self, ctx: &mut InteractiveContext, keyword: &str) -> CommandResult {
        // Handle "TIN", "CUSTOM", "UNDO", "CLOSE", "CANCEL"
    }
    
    fn on_enter(&mut self, ctx: &mut InteractiveContext) -> CommandResult {
        // Accept current Z, move to next point
    }
    
    fn on_cancel(&mut self, ctx: &mut InteractiveContext) -> CommandResult {
        // Clean up rubber band, exit
    }
    
    fn prompt(&self) -> String {
        match self.state {
            CreateState::PickingFirstPoint => "Pick first point on TIN surface: ".into(),
            CreateState::PickingNextPoint => "Pick next point [Undo/Close/Cancel]: ".into(),
            CreateState::ConfirmingZ { tin_z, .. } => 
                format!("TIN Z = {:.3}. Accept [TIN] or enter custom Z [Custom/Undo]: ", tin_z),
            CreateState::Finished => "Feature line created.".into(),
        }
    }
}
```

### Rubber Band Visualization
- [ ] Draw temporary lines between picked points
- [ ] Show TIN Z at cursor position in real-time
- [ ] Highlight current segment with grade display

### OSNAP Integration
- [ ] Snap to TIN vertices (priority 1)
- [ ] Snap to TIN edges (priority 2)  
- [ ] Snap to TIN surface (priority 3)
- [ ] Entity snaps (end, mid, cen, etc.)

### Acceptance Criteria
- [ ] Click on TIN → see Z interpolated in real-time
- [ ] Type "125.5" → overrides TIN Z for that vertex
- [ ] Type "TIN" → accepts interpolated Z
- [ ] Type "UNDO" → removes last point
- [ ] Type "CLOSE" → closes feature line (if >2 pts)
- [ ] Press ESC → cancels, cleans up rubber band

---

## 1.3 Feature Line Data Model

### Objective
Define core `FeatureLine` type in `landsurvey` crate (host-free).

### File Structure
```
crates/landsurvey/src/featureline/
├── mod.rs              # Public API exports
├── entity.rs           # FeatureLine, FeatureVertex, ZSource
├── edit.rs             # Vertex manipulation ops
├── grade.rs            # Grade/slope calculations
├── surface_link.rs     # Dynamic surface connection
└── breakline.rs        # Constrained triangulation (Phase 3)
```

### Core Types

```rust
// entity.rs
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeatureLine {
    pub name: String,
    pub vertices: Vec<FeatureVertex>,
    pub style: FeatureLineStyle,
    pub surface_link: Option<SurfaceLink>,
    pub breakline_type: BreaklineType,
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeatureVertex {
    pub pt: Point3d,           // X, Y, Z
    pub bulge: f64,            // Segment bulge (tan(sweep/4))
    pub z_source: ZSource,     // How Z was determined
    pub grade_in: Option<f64>,  // Grade from prev vertex (%)
    pub grade_out: Option<f64>, // Grade to next vertex (%)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ZSource {
    TINInterpolated,    // From surface at creation
    UserEntered,        // Manual override
    GradeCalculated,    // From grade tool
    BreaklineProjected, // From breakline projection
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceLink {
    pub surface_name: String,
    pub surface_handle: u64,
    pub link_mode: LinkMode,
    pub vertex_map: Vec<VertexLink>,  // Maps feature vertex → surface location
    pub last_sync: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LinkMode {
    Dynamic,      // Auto-update on surface change
    Static,       // Snapshot at creation
    Breakline,    // Feature line IS a breakline in surface
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VertexLink {
    pub vertex_index: usize,
    pub surface_uv: (f64, f64),  // Barycentric or parametric coords
    pub triangle_idx: usize,     // Containing triangle at link time
}
```

### Feature Line Style

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeatureLineStyle {
    pub layer: String,
    pub color: Color,
    pub linetype: String,
    pub lineweight: LineWeight,
    pub label_style: Option<String>,  // Elevation/grade label style
    pub show_vertices: bool,
    pub show_grades: bool,
}
```

### Serialization
- Implement `Serialize`/`Deserialize` for all types
- Store in `LANDSURVEY_FEATURELINE` XDATA as JSON
- Version field for migration

### Registry (in `crate::state`)
```rust
pub struct FeatureLineRegistry {
    lines: HashMap<String, FeatureLine>,
    by_handle: HashMap<Handle, String>,  // handle -> name
}

impl FeatureLineRegistry {
    pub fn put(&mut self, fl: FeatureLine) { /* ... */ }
    pub fn get(&self, name: &str) -> Option<&FeatureLine> { /* ... */ }
    pub fn get_by_handle(&self, handle: Handle) -> Option<&FeatureLine> { /* ... */ }
    pub fn remove(&mut self, name: &str) -> Option<FeatureLine> { /* ... */ }
    pub fn list(&self) -> Vec<&FeatureLine> { /* ... */ }
}
```

### Acceptance Criteria
- [ ] Create `FeatureLine` with 5 vertices, mixed Z sources
- [ ] Serialize to JSON, deserialize, verify equality
- [ ] Store in XDATA, retrieve, verify round-trip
- [ ] Registry: put/get/remove by name and handle
- [ ] All types implement `Serialize`/`Deserialize`

---

## 1.4 Persistence & Round-trip

### XDATA Format
```json
{
  "version": 1,
  "name": "FL-001",
  "vertices": [
    {"x": 1000.0, "y": 2000.0, "z": 125.5, "bulge": 0.0, "z_source": "TINInterpolated"},
    {"x": 1050.0, "y": 2000.0, "z": 126.0, "bulge": 0.0, "z_source": "UserEntered"}
  ],
  "style": {"layer": "LS-FEATURELINE", "color": 1, "linetype": "CONTINUOUS"},
  "surface_link": {
    "surface_name": "EG",
    "surface_handle": 12345,
    "link_mode": "Dynamic",
    "vertex_map": [{"vertex_index": 0, "surface_uv": [0.3, 0.4], "triangle_idx": 42}]
  },
  "breakline_type": "Standard"
}
```

### DWG/DXF Round-trip
- [ ] Register `LANDSURVEY_FEATURELINE` APPID on load
- [ ] Write XDATA on entity creation/modification
- [ ] Read XDATA on entity load, reconstruct `FeatureLine`
- [ ] Handle entity copy/paste/transform (update handle mapping)

### Acceptance Criteria
- [ ] Create feature line → save DWG → reopen → feature line intact
- [ ] Copy/paste feature line → new handle, new registry entry
- [ ] Transform (move/rotate/scale) → XDATA updated correctly
- [ ] Export DXF → import in Civil 3D → readable (LandXML fallback)

---

## Testing Strategy

### Unit Tests (landsurvey crate)
```rust
#[test]
fn feature_line_round_trip() {
    let fl = FeatureLine { /* ... */ };
    let json = serde_json::to_string(&fl).unwrap();
    let fl2: FeatureLine = serde_json::from_str(&json).unwrap();
    assert_eq!(fl, fl2);
}

#[test]
fn feature_line_xdata_round_trip() {
    let fl = FeatureLine::new(/* ... */);
    let xdata = fl.to_xdata();
    let fl2 = FeatureLine::from_xdata(&xdata).unwrap();
    assert_eq!(fl, fl2);
}

#[test]
fn vertex_z_source_preserved() {
    let mut fl = FeatureLine::new();
    fl.vertices[0].z_source = ZSource::UserEntered;
    // ... serialize/deserialize ...
    assert_eq!(fl.vertices[0].z_source, ZSource::UserEntered);
}
```

### Integration Tests (plugin crate)
```rust
#[test]
fn feature_line_create_from_csv() {
    let csv = "1000,2000\n1050,2000\n1100,2050";
    // ... run LS_FEATURELINE_CREATE csv ...
    // Verify entity created, XDATA correct
}

#[test]
fn feature_line_xdata_round_trip_dwg() {
    let fl = create_test_feature_line();
    let dwg = write_dwg_with_feature_line(&fl);
    let fl2 = read_feature_line_from_dwg(&dwg);
    assert_eq!(fl, fl2);
}
```

---

## Phase 1 Deliverables

| Deliverable | File | Status |
|-------------|------|--------|
| 3D Polyline support | acadrust PR / fork | ☐ |
| FeatureLine entity types | `crates/landsurvey/src/featureline/entity.rs` | ☐ |
| FeatureLine registry | `crates/opencad-landsurvey-plugin/src/state.rs` | ☐ |
| XDATA serialization | `crates/landsurvey/src/featureline/entity.rs` | ☐ |
| InteractiveCommand impl | `crates/opencad-landsurvey-plugin/src/interactive/featureline_create.rs` | ☐ |
| Point picking workflow | `crates/opencad-landsurvey-plugin/src/interactive/pick.rs` | ☐ |
| OSNAP integration | `crates/opencad-landsurvey-plugin/src/interactive/osnap.rs` | ☐ |
| Unit tests | `crates/landsurvey/src/featureline/entity.rs` tests | ☐ |
| Integration tests | `crates/opencad-landsurvey-plugin/src/dispatch.rs` tests | ☐ |

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| acadrust 3D polyline delayed | Implement XDATA-based fallback; migrate when ready |
| InteractiveCommand API changes | Pin OCS API version; use feature flags |
| Performance with rubber band | Limit rubber band to last 50 segments |
| XDATA size limits | Chunk large feature lines across multiple records |

---

## Dependencies

- **acadrust**: 3D polyline support (track `HakanSeven12/cadcodec#XXX`)
- **OCS Host API**: `InteractiveCommand` trait (v2+)
- **OCS Notifications**: Full-duplex for reactor (v4+)

---

*Next: Phase 2 - Interactive Editing Technical Plan*