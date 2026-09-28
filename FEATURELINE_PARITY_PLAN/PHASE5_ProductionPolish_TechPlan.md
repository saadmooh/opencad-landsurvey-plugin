# Phase 5: Production Polish - Technical Implementation Plan

**Duration**: 2-3 months  
**Target Parity**: 100% Civil 3D  
**Dependencies**: Phases 1-4 complete

---

## 5.1 Styles & Standards

### 5.1.1 Feature Line Style System

```rust
// crates/landsurvey/src/featureline/style.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeatureLineStyle {
    pub name: String,
    pub description: String,
    pub layer: String,
    pub color: EntityColor,
    pub linetype: String,
    pub lineweight: LineWeight,
    pub plot_style: Option<String>,
    // Vertex display
    pub show_vertices: bool,
    pub vertex_marker: VertexMarker,
    pub vertex_size: f64,
    // Grade display
    pub show_grades: bool,
    pub grade_precision: u8,        // Decimal places
    pub grade_format: GradeFormat,
    pub grade_color: EntityColor,
    // Elevation display
    pub show_elevations: bool,
    pub elevation_precision: u8,
    pub elevation_color: EntityColor,
    // Label style
    pub label_style: Option<String>,  // Reference to label style
    pub show_station: bool,
    pub station_interval: f64,
    pub station_precision: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VertexMarker {
    None,
    Square,
    Circle,
    Diamond,
    Cross,
}

impl Default for FeatureLineStyle {
    fn default() -> Self {
        Self {
            name: "Standard".into(),
            description: "Default feature line style".into(),
            layer: "LS-FEATURELINE".into(),
            color: EntityColor::ByLayer,
            linetype: "CONTINUOUS".into(),
            lineweight: LineWeight::ByLayer,
            plot_style: None,
            show_vertices: true,
            vertex_marker: VertexMarker::Square,
            vertex_size: 1.0,
            show_grades: true,
            grade_precision: 2,
            grade_format: GradeFormat::Percent,
            grade_color: EntityColor::ByLayer,
            show_elevations: true,
            elevation_precision: 2,
            elevation_color: EntityColor::ByLayer,
            label_style: None,
            show_station: true,
            station_interval: 10.0,
            station_precision: 2,
        }
    }
}

/// Style manager with persistence
pub struct StyleManager {
    styles: HashMap<String, FeatureLineStyle>,
    current: String,
}

impl StyleManager {
    pub fn new() -> Self {
        let mut mgr = Self {
            styles: HashMap::new(),
            current: "Standard".into(),
        };
        mgr.styles.insert("Standard".into(), FeatureLineStyle::default());
        mgr
    }
    
    pub fn get(&self, name: &str) -> Option<&FeatureLineStyle> { /* ... */ }
    pub fn get_current(&self) -> &FeatureLineStyle { /* ... */ }
    pub fn set_current(&mut self, name: &str) -> Result<()> { /* ... */ }
    pub fn add(&mut self, style: FeatureLineStyle) { /* ... */ }
    pub fn remove(&mut self, name: &str) -> Result<()> { /* ... */ }
    pub fn list(&self) -> Vec<&FeatureLineStyle> { /* ... */ }
    
    /// Import from XML/JSON
    pub fn import(&mut self, data: &str, format: StyleFormat) -> Result<()> { /* ... */ }
    
    /// Export to XML/JSON
    pub fn export(&self, names: &[&str], format: StyleFormat) -> Result<String> { /* ... */ }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StyleFormat {
    JSON,
    XML,      // Civil 3D compatible
    FLX,      // Autodesk Feature Line Exchange
}
```

### 5.1.2 Label Engine

```rust
// crates/landsurvey/src/featureline/label.rs

use crate::surface::Surface;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LabelStyle {
    pub name: String,
    pub text_style: String,          // Text style name
    pub text_height: f64,
    pub color: EntityColor,
    pub layer: String,
    pub offset: f64,                 // Distance from feature line
    pub orientation: LabelOrientation,
    pub content: LabelContent,
    pub format: LabelFormat,
    // Collision avoidance
    pub avoid_collisions: bool,
    pub min_gap: f64,
    pub max_stack: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LabelOrientation {
    Parallel,        // Parallel to segment
    Perpendicular,   // Perpendicular to segment
    Horizontal,      // Always horizontal
    Upright,         // Always readable (flipped if upside down)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LabelContent {
    Station,
    Elevation,
    Grade,
    StationElevation,
    StationGrade,
    StationElevationGrade,
    Custom(String),  // Template string
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LabelFormat {
    Decimal { precision: u8 },
    Fractional { denominator: u16 },
    FeetInches { precision: u8 },
    BearingDistance,
}

pub struct LabelEngine {
    style: LabelStyle,
}

impl LabelEngine {
    pub fn new(style: LabelStyle) -> Self { /* ... */ }
    
    /// Generate labels for feature line
    pub fn generate_labels(&self, fl: &FeatureLine, surface: Option<&Surface>) 
        -> Vec<LabelEntity> { /* ... */ }
    
    /// Generate station labels at interval
    pub fn station_labels(&self, fl: &FeatureLine, interval: f64) 
        -> Vec<LabelEntity> { /* ... */ }
    
    /// Generate elevation labels at vertices
    pub fn elevation_labels(&self, fl: &FeatureLine) 
        -> Vec<LabelEntity> { /* ... */ }
    
    /// Generate grade labels on segments
    pub fn grade_labels(&self, fl: &FeatureLine) 
        -> Vec<LabelEntity> { /* ... */ }
    
    /// Resolve collisions (move, stack, hide)
    fn resolve_collisions(&self, labels: &mut [LabelEntity]) { /* ... */ }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LabelEntity {
    pub position: Point3d,
    pub text: String,
    pub rotation: f64,      // Radians
    pub height: f64,
    pub style: LabelStyle,
    pub anchor: LabelAnchor,
    pub feature_line: String,
    pub vertex_idx: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LabelAnchor {
    TopLeft, TopCenter, TopRight,
    MiddleLeft, MiddleCenter, MiddleRight,
    BottomLeft, BottomCenter, BottomRight,
}
```

### Command: `LS_FEATURELINE_STYLE`

```rust
fn featureline_style(host: &mut dyn HostApi, cmd: &str) {
    let args = parse_args(cmd);
    match args.first().map(|s| s.to_uppercase()).as_deref() {
        Some("LIST") => list_styles(host),
        Some("SET") => set_current_style(host, &args[1]),
        Some("NEW") => create_style_interactive(host),
        Some("EDIT") => edit_style_interactive(host, &args[1]),
        Some("IMPORT") => import_styles(host, &args[1]),
        Some("EXPORT") => export_styles(host, &args[1..]),
        Some("DELETE") => delete_style(host, &args[1]),
        _ => host.push_info("Usage: LS_FEATURELINE_STYLE [LIST|SET|NEW|EDIT|IMPORT|EXPORT|DELETE]"),
    }
}
```

### Acceptance Criteria
- [ ] Create custom style with all properties
- [ ] Apply style to feature line → visual update
- [ ] Labels generated at stations, elevations, grades
- [ ] Collision avoidance works (no overlapping labels)
- [ ] Import/Export Civil 3D XML format
- [ ] Style persists in XDATA on feature line entity

---

## 5.2 Data Management

### 5.2.1 Feature Line Groups/Sites

```rust
// crates/landsurvey/src/featureline/group.rs

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeatureLineGroup {
    pub name: String,
    pub description: String,
    pub member_names: Vec<String>,  // Feature line names
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
    pub color: Option<EntityColor>,  // Group display color
    pub locked: bool,
}

pub struct GroupManager {
    groups: HashMap<String, FeatureLineGroup>,
}

impl GroupManager {
    pub fn create_group(&mut self, name: &str, members: &[String]) -> Result<()> { /* ... */ }
    pub fn add_to_group(&mut self, group: &str, members: &[String]) -> Result<()> { /* ... */ }
    pub fn remove_from_group(&mut self, group: &str, members: &[String]) -> Result<()> { /* ... */ }
    pub fn delete_group(&mut self, name: &str) -> Result<()> { /* ... */ }
    pub fn isolate_group(&self, name: &str) -> Vec<&FeatureLine> { /* ... */ }
    pub fn batch_operation<F>(&self, group: &str, op: F) -> Result<()> 
        where F: Fn(&mut FeatureLine) -> Result<()> { /* ... */ }
}
```

### 5.2.2 Site/Parcel Integration

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Site {
    pub name: String,
    pub parcels: Vec<Parcel>,
    pub feature_line_groups: Vec<String>,
    pub alignment_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Parcel {
    pub name: String,
    pub boundary: FeatureLine,  // Closed feature line
    pub area: f64,
    pub label_style: String,
}
```

### 5.2.3 Import/Export

```rust
// LandXML import/export for feature lines
pub fn export_landxml_feature_lines(fls: &[FeatureLine]) -> Result<String> { /* ... */ }
pub fn import_landxml_feature_lines(xml: &str) -> Result<Vec<FeatureLine>> { /* ... */ }

// Civil 3D FLX format (Feature Line Exchange)
pub fn export_flx(fls: &[FeatureLine]) -> Result<Vec<u8>> { /* ... */ }
pub fn import_flx(data: &[u8]) -> Result<Vec<FeatureLine>> { /* ... */ }

// CSV/GeoJSON for GIS
pub fn export_geojson(fls: &[FeatureLine]) -> Result<String> { /* ... */ }
pub fn import_geojson(json: &str) -> Result<Vec<FeatureLine>> { /* ... */ }
```

### Acceptance Criteria
- [ ] Create feature line groups
- [ ] Batch operations on groups (style, grade, offset)
- [ ] Export/Import LandXML with feature lines
- [ ] Export/Import FLX format (Civil 3D round-trip)
- [ ] Export GeoJSON for GIS
- [ ] Site/parcel structure for civil design

---

## 5.3 Performance & Scale

### 5.3.1 Spatial Indexing

```rust
// crates/landsurvey/src/featureline/spatial.rs

use rstar::{RTree, RTreeObject, AABB, PointDistance};

#[derive(Clone)]
pub struct SpatialIndex {
    tree: RTree<FeatureLineSpatialObject>,
}

#[derive(Clone)]
struct FeatureLineSpatialObject {
    fl_name: String,
    vertex_idx: usize,
    point: [f64; 2],  // XY for 2D tree
}

impl RTreeObject for FeatureLineSpatialObject {
    type Envelope = AABB<[f64; 2]>;
    fn envelope(&self) -> Self::Envelope { /* point as AABB */ }
}

impl PointDistance for FeatureLineSpatialObject {
    fn distance_2(&self, point: &[f64; 2]) -> f64 {
        let dx = self.point[0] - point[0];
        let dy = self.point[1] - point[1];
        dx * dx + dy * dy
    }
}

impl FeatureLine {
    pub fn spatial_index(&self) -> SpatialIndex { /* ... */ }
    
    /// Find nearest vertex to point (uses spatial index)
    pub fn nearest_vertex(&self, index: &SpatialIndex, pt: Point2d, max_dist: f64) 
        -> Option<(usize, f64)> { /* ... */ }
    
    /// Find all vertices within distance
    pub fn vertices_within(&self, index: &SpatialIndex, pt: Point2d, dist: f64) 
        -> Vec<(usize, f64)> { /* ... */ }
}
```

### 5.3.2 Lazy Loading & Viewport Culling

```rust
pub struct FeatureLineRenderer {
    full_fl: FeatureLine,
    // Level of detail
    lod_threshold: usize,  // If vertices > threshold, simplify
    simplified_cache: Option<FeatureLine>,
}

impl FeatureLineRenderer {
    pub fn get_vertices_for_viewport(&self, viewport: &Viewport) -> &[FeatureVertex] {
        if self.full_fl.vertices.len() > self.lod_threshold {
            self.simplified_cache.get_or_insert_with(|| self.simplify(0.5))
        } else {
            &self.full_fl.vertices
        }
    }
    
    fn simplify(&self, tolerance: f64) -> FeatureLine { /* Douglas-Peucker */ }
}
```

### 5.3.3 Undo/Redo Optimization

```rust
pub struct FeatureLineHistory {
    snapshots: Vec<FeatureLineSnapshot>,
    current: usize,
    max_snapshots: usize,
}

#[derive(Clone)]
struct FeatureLineSnapshot {
    fl: FeatureLine,
    timestamp: DateTime<Utc>,
    description: String,
    // Coalesced operations
    operation_type: OperationType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OperationType {
    VertexMove,
    VertexInsert,
    VertexDelete,
    GradeApply,
    StyleChange,
    PropertyChange,
}

impl FeatureLineHistory {
    pub fn push(&mut self, fl: &FeatureLine, desc: &str, op: OperationType) { /* ... */ }
    pub fn undo(&mut self) -> Option<FeatureLine> { /* ... */ }
    pub fn redo(&mut self) -> Option<FeatureLine> { /* ... */ }
    
    /// Coalesce consecutive same-type operations
    fn coalesce(&mut self) { /* ... */ }
}
```

### Acceptance Criteria
- [ ] 10,000 vertex feature line: snapping < 10ms
- [ ] Viewport culling: 10k vertices → 500 rendered at 60fps
- [ ] Undo/redo 1000 ops < 50ms
- [ ] Memory < 100MB for 100 feature lines @ 10k vertices each

---

## 5.4 Documentation & Testing

### 5.4.1 User Guide

```markdown
# Feature Line User Guide

## Getting Started
1. Create TIN surface (LS_SURFACE or LS_LANDXML)
2. Click "Feature Line" button in Land Survey ribbon
3. Click points on surface to create feature line
4. Press Enter to accept TIN Z, or type custom Z

## Editing Feature Lines
- Elevation Editor: LS_FEATURELINE_ELEVATIONS
- Vertex editing: LS_FEATURELINE_EDIT
- Grade tools: LS_FEATURELINE_GRADE
- Offset/Daylight: LS_FEATURELINE_OFFSET, LS_FEATURELINE_DAYLIGHT

## Grading
- Create grading: LS_GRADING_CREATE
- Report: LS_GRADING_REPORT

## Styles
- Manage styles: LS_FEATURELINE_STYLE
- Import/Export Civil 3D styles

## Civil 3D Workflow Mapping
| Civil 3D | OpenCADStudio |
|----------|---------------|
| Feature Line | LS_FEATURELINE_CREATE |
| Elevation Editor | LS_FEATURELINE_ELEVATIONS |
| Set Grade | LS_FEATURELINE_GRADE |
| Offset | LS_FEATURELINE_OFFSET |
| Daylight | LS_FEATURELINE_DAYLIGHT |
| Grading Group | LS_GRADING_CREATE |
```

### 5.4.2 Automated Testing

```rust
// crates/landsurvey/src/featureline/tests.rs

mod golden_tests {
    use super::*;
    
    #[test]
    fn civil3d_feature_line_round_trip() {
        // Load Civil 3D exported FLX
        // Import, verify all properties
        // Export, compare with original
    }
    
    #[test]
    fn civil3d_grading_volume_accuracy() {
        // Load Civil 3D grading project
        // Compute volumes
        // Compare with Civil 3D reported volumes
        // Tolerance: < 0.1% difference
    }
    
    #[test]
    fn feature_line_performance_10k_vertices() {
        let fl = generate_feature_line(10000);
        let start = Instant::now();
        let _ = fl.spatial_index();
        assert!(start.elapsed() < Duration::from_millis(50));
    }
    
    #[test]
    fn feature_line_snapping_10k() {
        let fl = generate_feature_line(10000);
        let idx = fl.spatial_index();
        let start = Instant::now();
        let _ = fl.nearest_vertex(&idx, [5000.0, 5000.0], 100.0);
        assert!(start.elapsed() < Duration::from_millis(10));
    }
    
    #[test]
    fn grade_accuracy_vs_civil3d() {
        // Known geometry from Civil 3D
        // Apply grade, compare elevations
        // Tolerance: 0.001 ft
    }
}
```

### 5.4.3 CI/CD Pipeline

```yaml
# .github/workflows/featureline-tests.yml
name: Feature Line Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - name: Install Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      - name: Run landsurvey tests
        run: cargo test -p landsurvey
      - name: Run plugin tests
        run: cargo test -p opencad-landsurvey-plugin
      - name: Run golden tests
        run: cargo test -p landsurvey --test golden
      - name: Benchmark
        run: cargo bench -p landsurvey -- featureline
```

### Acceptance Criteria
- [ ] User guide covers all commands with Civil 3D mapping
- [ ] Golden tests pass (Civil 3D round-trip)
- [ ] Performance benchmarks pass (< 10ms snap, < 50ms 10k index)
- [ ] CI pipeline runs on every PR
- [ ] Video tutorials for key workflows

---

## 5.5 Civil 3D Round-trip Validation

### 5.5.1 Test Matrix

| Feature | Civil 3D → OCS | OCS → Civil 3D | Status |
|---------|----------------|----------------|--------|
| Basic feature line | ✅ | ✅ | ✅ |
| 3D vertices with Z | ✅ | ✅ | ✅ |
| Bulge/arcs | ✅ | ✅ | ✅ |
| Elevation overrides | ✅ | ✅ | ✅ |
| Grade values | ✅ | ✅ | 🟡 |
| Styles | ❌ | ❌ | 🟡 |
| Labels | ❌ | ❌ | 🟡 |
| Breakline link | ✅ | ✅ | 🟡 |
| Grading surface | ❌ | ❌ | 🟡 |
| Styles import/export | ✅ | ✅ | 🟡 |

### 5.5.2 Golden Files

```
tests/golden/
├── civil3d/
│   ├── simple_featureline.flx
│   ├── featureline_with_arcs.flx
│   ├── featureline_with_grades.flx
│   ├── grading_project.flx
│   └── site_with_parcels.flx
└── expected/
    ├── simple_featureline.json
    ├── featureline_with_arcs.json
    └── ...
```

---

## Phase 5 Deliverables

| Deliverable | File | Status |
|-------------|------|--------|
| Style system | `crates/landsurvey/src/featureline/style.rs` | ☐ |
| Label engine | `crates/landsurvey/src/featureline/label.rs` | ☐ |
| Style manager | `crates/opencad-landsurvey-plugin/src/style_manager.rs` | ☐ |
| Label command | `crates/opencad-landsurvey-plugin/src/dispatch.rs` | ☐ |
| Group/Site management | `crates/landsurvey/src/featureline/group.rs` | ☐ |
| LandXML/FLX/GeoJSON I/O | `crates/landsurvey/src/featureline/io.rs` | ☐ |
| Spatial index | `crates/landsurvey/src/featureline/spatial.rs` | ☐ |
| LOD/Renderer | `crates/landsurvey/src/featureline/renderer.rs` | ☐ |
| Undo/Redo optimization | `crates/landsurvey/src/featureline/history.rs` | ☐ |
| User guide | `docs/FEATURELINE_GUIDE.md` | ☐ |
| Golden tests | `crates/landsurvey/tests/golden/` | ☐ |
| CI/CD pipeline | `.github/workflows/featureline-tests.yml` | ☐ |

---

## Final Validation Checklist

### Civil 3D Parity Verification
- [ ] All 25 Civil 3D feature line commands mapped
- [ ] All 15 XDATA properties round-trip
- [ ] All 8 grading tools functional
- [ ] 3D polyline native support
- [ ] Dynamic surface link reactive
- [ ] Breakline integration functional
- [ ] Style system complete
- [ ] Labels with collision avoidance
- [ ] LandXML/FLX round-trip > 99%
- [ ] Performance: 10k vertices < 10ms snap
- [ ] Memory: 100 FL @ 10k verts < 100MB
- [ ] Undo/redo 1000 ops < 50ms

### Documentation
- [ ] User guide with Civil 3D mapping table
- [ ] API reference for `landsurvey::featureline`
- [ ] Video tutorials for 5 key workflows
- [ ] Migration guide from Civil 3D

### Quality Gates
- [ ] All tests pass (unit + integration + golden)
- [ ] Cargo clippy clean
- [ ] Cargo fmt clean
- [ ] Benchmarks pass
- [ ] Civil 3D round-trip test suite passes

---

*End of Phase 5 Technical Plan*