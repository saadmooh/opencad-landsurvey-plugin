# Phase 2: Interactive Editing - Technical Implementation Plan

**Duration**: 3-4 months  
**Target Parity**: 55% Civil 3D  
**Dependencies**: Phase 1 complete (FeatureLine entity, interactive picking)

---

## 2.1 Elevation Editor (Tabular Grid)

### Objective
Full-featured tabular editor for per-vertex elevation management.

### Architecture

```rust
// crates/landsurvey/src/featureline/edit.rs

pub struct ElevationEditor {
    feature_line: FeatureLine,
    edit_history: Vec<EditAction>,
    undo_stack: Vec<EditSnapshot>,
    redo_stack: Vec<EditSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditAction {
    pub vertex_idx: usize,
    pub field: EditField,
    pub old_value: f64,
    pub new_value: f64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditField {
    Elevation,
    GradeIn,
    GradeOut,
    Bulge,
}

impl ElevationEditor {
    pub fn new(fl: FeatureLine) -> Self { /* ... */ }
    
    pub fn set_elevation(&mut self, idx: usize, z: f64) -> Result<()> { /* ... */ }
    pub fn set_grade(&mut self, idx: usize, grade_in: Option<f64>, grade_out: Option<f64>) -> Result<()> { /* ... */ }
    pub fn set_bulge(&mut self, idx: usize, bulge: f64) -> Result<()> { /* ... */ }
    
    pub fn apply_constant_grade(&mut self, start: usize, end: usize, grade: f64) -> Result<()> { /* ... */ }
    pub fn smooth(&mut self, window: usize) -> Result<()> { /* ... */ }
    pub fn flatten(&mut self, start: usize, end: usize, z: f64) -> Result<()> { /* ... */ }
    
    pub fn undo(&mut self) -> bool { /* ... */ }
    pub fn redo(&mut self) -> bool { /* ... */ }
    
    pub fn get_table(&self) -> Vec<ElevationRow> { /* ... */ }
    pub fn commit(&mut self) -> FeatureLine { /* ... */ }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElevationRow {
    pub idx: usize,
    pub easting: f64,
    pub northing: f64,
    pub elevation: f64,
    pub z_source: ZSource,
    pub grade_in: Option<f64>,   // % from prev vertex
    pub grade_out: Option<f64>,  // % to next vertex
    pub distance_3d: f64,        // 3D distance from prev
    pub distance_2d: f64,        // 2D distance from prev
}
```

### UI Integration (via OCS InteractiveCommand)
```rust
pub struct ElevationEditorCommand {
    editor: ElevationEditor,
    selected_row: Option<usize>,
    edit_mode: EditMode,
}

enum EditMode {
    View,
    EditCell { row: usize, col: Column },
    BulkEdit { range: Range<usize>, op: BulkOp },
}

impl InteractiveCommand for ElevationEditorCommand {
    fn on_keyword(&mut self, ctx: &mut InteractiveContext, kw: &str) -> CommandResult {
        match kw {
            "SET" => self.enter_edit_mode(),
            "GRADE" => self.set_grade_range(),
            "SMOOTH" => self.smooth_elevations(),
            "FLATTEN" => self.flatten_range(),
            "UNDO" => self.undo(),
            "REDO" => self.redo(),
            "DONE" => self.commit(),
            "CANCEL" => self.cancel(),
            _ => CommandResult::Ignore,
        }
    }
}
```

### Acceptance Criteria
- [ ] Open editor → displays table with all vertices
- [ ] Edit single cell → updates elevation, recalculates grades
- [ ] Set grade range → interpolates elevations between vertices
- [ ] Smooth operation → reduces elevation variance
- [ ] Flatten range → sets constant elevation
- [ ] Undo/Redo stack works correctly
- [ ] Commit returns updated FeatureLine

---

## 2.2 Vertex Manipulation

### Core Operations

```rust
// crates/landsurvey/src/featureline/edit.rs

impl FeatureLine {
    /// Insert vertex at parameter t along segment [idx, idx+1]
    pub fn insert_vertex(&mut self, segment_idx: usize, t: f64) -> Result<usize> {
        // t in (0,1): position along segment
        // Interpolate XY, Z from adjacent vertices or TIN
    }
    
    /// Delete vertex at index (must keep >= 2 vertices)
    pub fn delete_vertex(&mut self, idx: usize) -> Result<()> { /* ... */ }
    
    /// Move vertex to new XY, optionally update Z from TIN
    pub fn move_vertex(&mut self, idx: usize, new_xy: Point2d, update_z: bool, surface: Option<&Surface>) -> Result<()> { /* ... */ }
    
    /// Move vertex Z only (vertical grip)
    pub fn move_vertex_z(&mut self, idx: usize, new_z: f64) -> Result<()> { /* ... */ }
    
    /// Fillet at vertex idx with radius r
    pub fn fillet(&mut self, idx: usize, radius: f64) -> Result<()> { /* ... */ }
    
    /// Chamfer at vertex idx with distance d
    pub fn chamfer(&mut self, idx: usize, distance: f64) -> Result<()> { /* ... */ }
    
    /// Reverse vertex order
    pub fn reverse(&mut self) { /* ... */ }
    
    /// Join with another feature line (end-to-end)
    pub fn join(&mut self, other: FeatureLine) -> Result<()> { /* ... */ }
    
    /// Split at vertex or distance along line
    pub fn split(&self, at: SplitLocation) -> Result<(FeatureLine, FeatureLine)> { /* ... */ }
    
    /// Trim to intersection with another entity
    pub fn trim(&mut self, other: &EntityType) -> Result<()> { /* ... */ }
    
    /// Extend to intersection with another entity
    pub fn extend(&mut self, other: &EntityType) -> Result<()> { /* ... */ }
}

#[derive(Debug, Clone, Copy)]
pub enum SplitLocation {
    AtVertex(usize),
    AtDistance(f64),  // From start
    AtParameter(usize, f64),  // Segment idx + t in (0,1)
}
```

### Interactive Command Integration

```rust
pub struct VertexEditCommand {
    feature_line: FeatureLine,
    selected_idx: Option<usize>,
    edit_mode: VertexEditMode,
}

enum VertexEditMode {
    Select,           // Click to select vertex
    MoveXY,           // Drag selected vertex XY
    MoveZ,            // Vertical drag
    Insert,           // Click segment to insert
    Delete,           // Click vertex to delete
    Fillet,           // Click vertex + enter radius
    Chamfer,          // Click vertex + enter distance
}

impl InteractiveCommand for VertexEditCommand {
    fn on_pick(&mut self, ctx: &mut InteractiveContext, pt: Point3d) -> CommandResult {
        match self.edit_mode {
            VertexEditMode::Select => { /* select nearest vertex */ }
            VertexEditMode::MoveXY => { /* drag to new position */ }
            VertexEditMode::Insert => { /* insert at picked segment */ }
            _ => CommandResult::Ignore,
        }
    }
    
    fn on_keyword(&mut self, ctx: &mut InteractiveContext, kw: &str) -> CommandResult {
        match kw {
            "INSERT" => self.edit_mode = VertexEditMode::Insert,
            "DELETE" => self.delete_selected(),
            "FILLET" => self.fillet_mode(),
            "CHAMFER" => self.chamfer_mode(),
            "REVERSE" => self.reverse(),
            "JOIN" => self.join_mode(),
            "SPLIT" => self.split_mode(),
            "TRIM" => self.trim_mode(),
            "EXTEND" => self.extend_mode(),
            _ => CommandResult::Ignore,
        }
    }
}
```

### Acceptance Criteria
- [ ] Click segment → insert vertex at click location
- [ ] Click vertex → select it (highlight)
- [ ] Drag selected vertex → move XY, snap to TIN
- [ ] Type "Z 125.5" → set vertex Z
- [ ] "DELETE" → remove vertex (min 2 remaining)
- [ ] "FILLET 5" → click vertex → add fillet radius 5
- [ ] "REVERSE" → reverse vertex order
- [ ] "JOIN" → pick another feature line → join end-to-end
- [ ] "SPLIT" → click point → split into two feature lines

---

## 2.3 Geometry Editing: Join/Split/Trim/Extend

### Join
```rust
impl FeatureLine {
    pub fn join(&mut self, other: FeatureLine) -> Result<()> {
        // Check if end of self matches start of other (within tolerance)
        // or end of other matches start of self
        // Merge vertices, remove duplicate at junction
        // Merge surface links (union)
        // Merge styles (keep self's style)
    }
}
```

### Split
```rust
impl FeatureLine {
    pub fn split(&self, at: SplitLocation) -> Result<(FeatureLine, FeatureLine)> {
        match at {
            SplitLocation::AtVertex(idx) => {
                // Split vertices into [0..=idx] and [idx..]
                // Duplicate vertex at split point
            }
            SplitLocation::AtDistance(dist) => {
                // Find segment containing distance
                // Interpolate vertex at distance
                // Split at interpolated vertex
            }
            SplitLocation::AtParameter(seg_idx, t) => {
                // Interpolate on segment
                // Split at interpolated point
            }
        }
    }
}
```

### Trim/Extend
```rust
impl FeatureLine {
    pub fn trim(&mut self, other: &EntityType) -> Result<()> {
        // Find intersection with other entity
        // Remove portion before/after intersection
    }
    
    pub fn extend(&mut self, other: &EntityType) -> Result<()> {
        // Find intersection with other entity
        // Extend last/first segment to intersection
    }
}
```

### Acceptance Criteria
- [ ] Join two collinear feature lines → single feature line
- [ ] Join with small gap (< tolerance) → auto-close gap
- [ ] Split at vertex → two feature lines sharing vertex
- [ ] Split at distance → interpolated vertex, two feature lines
- [ ] Trim to line/circle/arc → remove portion
- [ ] Extend to line/circle/arc → extend to intersection

---

## 2.4 Grip Editing

### Grip Types

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GripType {
    Vertex,           // Square grip at vertex (move XY)
    VertexZ,          // Diamond grip at vertex (move Z only)
    Segment,          // Mid-segment grip (move segment)
    Bulge,            // Arc grip (modify bulge)
    GradeIn,          // Grade handle at vertex
    GradeOut,
}

pub struct Grip {
    pub grip_type: GripType,
    pub vertex_idx: usize,
    pub position: Point3d,
    pub is_active: bool,
}
```

### Grip Editing Command

```rust
pub struct GripEditCommand {
    feature_line: FeatureLine,
    active_grip: Option<Grip>,
    drag_start: Point3d,
}

impl InteractiveCommand for GripEditCommand {
    fn on_pick(&mut self, ctx: &mut InteractiveContext, pt: Point3d) -> CommandResult {
        // Find nearest grip to pick point
        // Activate that grip
    }
    
    fn on_drag(&mut self, ctx: &mut InteractiveContext, pt: Point3d) -> CommandResult {
        // Update grip position
        // Update feature line geometry
        // Redraw rubber band
    }
    
    fn on_release(&mut self, ctx: &mut InteractiveContext) -> CommandResult {
        // Commit grip movement
        // Record in undo stack
    }
}
```

### Grip Display
- [ ] Draw grips at vertices (square for XY, diamond for Z)
- [ ] Draw grade handles at vertices
- [ ] Highlight active grip
- [ ] Show tooltip with current values on hover

### Acceptance Criteria
- [ ] Click vertex → shows XY and Z grips
- [ ] Drag XY grip → moves vertex, snaps to TIN
- [ ] Drag Z grip → moves vertex vertically only
- [ ] Drag grade handle → adjusts grade, updates adjacent elevations
- [ ] Multi-select vertices → move together
- [ ] ESC cancels drag, restores original

---

## Testing Strategy

### Unit Tests (landsurvey)
```rust
#[test]
fn insert_vertex_interpolates_z() {
    let mut fl = straight_line();
    let idx = fl.insert_vertex(0, 0.5).unwrap();
    // Z should be interpolated from adjacent vertices
    assert!((fl.vertices[idx].z - 125.5).abs() < 0.001);
}

#[test]
fn delete_vertex_maintains_continuity() {
    let mut fl = three_point_line();
    fl.delete_vertex(1).unwrap();
    assert_eq!(fl.vertices.len(), 2);
    // Endpoints unchanged
}

#[test]
fn fillet_adds_curve() {
    let mut fl = right_angle_line();
    fl.fillet(1, 5.0).unwrap();
    // Vertex 1 now has bulge != 0
    assert!(fl.vertices[1].bulge > 0.0);
}

#[test]
fn split_at_vertex() {
    let fl = three_point_line();
    let (a, b) = fl.split(SplitLocation::AtVertex(1)).unwrap();
    assert_eq!(a.vertices.len(), 2);
    assert_eq!(b.vertices.len(), 2);
    // Shared vertex at split
}
```

### Integration Tests
```rust
#[test]
fn interactive_vertex_edit() {
    // Simulate interactive session
    // Pick vertex -> move -> accept -> verify
}

#[test]
fn grip_edit_moves_vertex() {
    // Simulate grip drag
    // Verify final position
}
```

---

## Phase 2 Deliverables

| Deliverable | File | Status |
|-------------|------|--------|
| ElevationEditor | `crates/landsurvey/src/featureline/edit.rs` | ☐ |
| ElevationEditorCommand | `crates/opencad-landsurvey-plugin/src/interactive/elevation_editor.rs` | ☐ |
| Vertex manipulation | `crates/landsurvey/src/featureline/edit.rs` | ☐ |
| VertexEditCommand | `crates/opencad-landsurvey-plugin/src/interactive/vertex_edit.rs` | ☐ |
| GripEditCommand | `crates/opencad-landsurvey-plugin/src/interactive/grip_edit.rs` | ☐ |
| Join/Split/Trim/Extend | `crates/landsurvey/src/featureline/edit.rs` | ☐ |
| Grip display | `crates/opencad-landsurvey-plugin/src/interactive/grip.rs` | ☐ |
| Unit tests | `crates/landsurvey/src/featureline/edit.rs` tests | ☐ |
| Integration tests | `crates/opencad-landsurvey-plugin/src/dispatch.rs` tests | ☐ |

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Complex undo/redo | Command pattern with snapshot-based history |
| Floating point precision | Use epsilon (1e-9) for comparisons |
| TIN snap conflicts | Priority: vertex > edge > surface |
| Large feature line performance | Spatial index (R-tree) for vertex queries |

---

## Dependencies
- Phase 1 complete (FeatureLine entity, interactive framework)
- OCS `InteractiveCommand` with drag support
- Spatial index library (R-tree) for large feature lines

---

*Next: Phase 3 - Dynamic Surface Link Technical Plan*