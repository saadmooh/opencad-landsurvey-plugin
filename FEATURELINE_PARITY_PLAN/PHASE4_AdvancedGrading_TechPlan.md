# Phase 4: Advanced Grading - Technical Implementation Plan

**Duration**: 3-4 months  
**Target Parity**: 90% Civil 3D  
**Dependencies**: Phase 3 complete (dynamic surface link, breaklines)

---

## 4.1 Grade/Slope Tools

### Objective
Implement Civil 3D-style grade/slope application between feature line vertices.

### Core Grade Engine

```rust
// crates/landsurvey/src/featureline/grade.rs

use crate::surface::Surface;
use std::collections::HashMap;

/// Grade/slope representation
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Grade {
    pub value: f64,        // As percentage (e.g., 2.0 = 2%)
    pub format: GradeFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GradeFormat {
    Percent,      // 2.0 = 2%
    Ratio,        // 50.0 = 50:1 (1:50)
    Degrees,      // Angle in degrees
    PerMille,     // Per mille (‰)
}

impl Grade {
    pub fn from_percent(p: f64) -> Self { Self { value: p, format: GradeFormat::Percent } }
    pub fn from_ratio(r: f64) -> Self { Self { value: r, format: GradeFormat::Ratio } }
    pub fn from_degrees(d: f64) -> Self { Self { value: d, format: GradeFormat::Degrees } }
    
    pub fn as_percent(&self) -> f64 {
        match self.format {
            GradeFormat::Percent => self.value,
            GradeFormat::Ratio => 100.0 / self.value,
            GradeFormat::Degrees => self.value.tan() * 100.0,
            GradeFormat::PerMille => self.value / 10.0,
        }
    }
    
    pub fn as_ratio(&self) -> f64 {
        100.0 / self.as_percent()
    }
    
    pub fn as_radians(&self) -> f64 {
        (self.as_percent() / 100.0).atan()
    }
}

/// Grade application result
#[derive(Debug, Clone)]
pub struct GradeApplicationResult {
    pub vertices_affected: Vec<usize>,
    pub elevations_changed: Vec<(usize, f64, f64)>, // (idx, old_z, new_z)
    pub grade_applied: Grade,
    pub direction: GradeDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GradeDirection {
    Forward,   // From start_idx toward end
    Backward,  // From end_idx toward start
    Both,      // From middle outward
}

impl FeatureLine {
    /// Apply constant grade from start_idx toward end
    /// grade: positive = uphill, negative = downhill
    pub fn apply_grade(&mut self, start_idx: usize, grade: Grade, 
                       direction: GradeDirection, surface: Option<&Surface>) 
        -> Result<GradeApplicationResult> {
        // 1. Validate start_idx
        // 2. Compute new elevations by propagating grade
        // 3. If surface provided, check against surface (conflict detection)
        // 4. Update vertices, mark z_source = GradeCalculated
        // 5. Update grade_in/grade_out for affected segments
    }
    
    /// Apply grade to hit target elevation at target_idx
    pub fn grade_to_elevation(&mut self, start_idx: usize, target_idx: usize, 
                               target_z: f64, surface: Option<&Surface>) 
        -> Result<GradeApplicationResult> {
        // Compute required grade to hit target_z at target_idx
        // Apply that grade
    }
    
    /// Apply grade to surface intersection
    /// Projects from start_idx at given grade until hitting surface
    pub fn grade_to_surface(&mut self, start_idx: usize, grade: Grade,
                             surface: &Surface, max_distance: f64) 
        -> Result<GradeApplicationResult> {
        // 1. Project ray from start_idx at grade
        // 2. Find intersection with surface
        // 3. Insert vertices along projection path
        // 4. Return result with intersection point
    }
    
    /// Set grade between two existing vertices
    pub fn set_grade_between(&mut self, idx1: usize, idx2: usize, 
                              grade: Grade) -> Result<GradeApplicationResult> {
        // Compute required elevations at idx1 and idx2
        // Interpolate intermediate vertices
    }
    
    /// Smooth grades over range (cubic spline)
    pub fn smooth_grades(&mut self, start: usize, end: usize, 
                          tension: f64) -> Result<()> { /* ... */ }
    
    /// Balance cut/fill over range (adjust grades to balance volumes)
    pub fn balance_grades(&mut self, start: usize, end: usize, 
                           target_surface: &Surface) -> Result<()> { /* ... */ }
}

/// Grade interpolation helper
fn interpolate_grade(z1: f64, z2: f64, dist_2d: f64) -> Grade {
    let percent = ((z2 - z1) / dist_2d) * 100.0;
    Grade::from_percent(percent)
}

fn apply_grade(z1: f64, grade: Grade, dist_2d: f64) -> f64 {
    z1 + (grade.as_percent() / 100.0) * dist_2d
}
```

### Interactive Command: `LS_FEATURELINE_GRADE`

```rust
// crates/opencad-landsurvey-plugin/src/interactive/grade_tool.rs

pub struct GradeToolCommand {
    feature_line: FeatureLine,
    mode: GradeMode,
    start_idx: Option<usize>,
    grade: Option<Grade>,
}

enum GradeMode {
    SelectStart,      // Pick start vertex
    EnterGrade,       // Input grade value
    SelectDirection,  // Forward/Backward/Both
    SelectTarget,     // Pick target vertex or surface
    Preview,          // Show grade line rubber band
    Confirm,
}

impl InteractiveCommand for GradeToolCommand {
    fn prompt(&self) -> String {
        match self.mode {
            GradeMode::SelectStart => "Select start vertex for grade: ".into(),
            GradeMode::EnterGrade => "Enter grade (e.g., 2%, 3:1, 5deg): ".into(),
            GradeMode::SelectDirection => "Direction [Forward/Backward/Both]: ".into(),
            GradeMode::SelectTarget => "Select target vertex or surface [Surface/Vertex]: ".into(),
            GradeMode::Preview => "Grade preview shown. Accept [Yes/No/Modify]: ".into(),
            GradeMode::Confirm => "Apply grade? [Yes/No]: ".into(),
        }
    }
    
    fn on_pick(&mut self, ctx: &mut InteractiveContext, pt: Point3d) -> CommandResult {
        match self.mode {
            GradeMode::SelectStart => {
                // Find nearest vertex
                self.start_idx = Some(idx);
                self.mode = GradeMode::EnterGrade;
            }
            GradeMode::SelectTarget => {
                // If picked surface, switch to grade_to_surface mode
                // If picked vertex, use as target_idx
            }
            _ => CommandResult::Ignore,
        }
    }
    
    fn on_keyword(&mut self, ctx: &mut InteractiveContext, kw: &str) -> CommandResult {
        match kw.to_uppercase().as_str() {
            "PERCENT" => { /* parse as % */ }
            "RATIO" => { /* parse as ratio */ }
            "DEG" | "DEGREES" => { /* parse as degrees */ }
            "FORWARD" => self.direction = GradeDirection::Forward,
            "BACKWARD" => self.direction = GradeDirection::Backward,
            "BOTH" => self.direction = GradeDirection::Both,
            "SURFACE" => { /* switch to grade_to_surface */ }
            "VERTEX" => { /* switch to grade_to_vertex */ }
            "PREVIEW" => { self.show_preview(); }
            "APPLY" => self.apply_grade(),
            "UNDO" => self.undo_last(),
            _ => CommandResult::Ignore,
        }
    }
}
```

### Acceptance Criteria
- [ ] `LS_FEATURELINE_GRADE` → pick vertex → enter "2%" → select Forward → applies 2% grade forward
- [ ] Enter "3:1" → interpreted as 3:1 ratio (33.33%)
- [ ] Enter "5deg" → interpreted as 5 degrees
- [ ] Select target vertex → computes required grade to hit that vertex
- [ ] "SURFACE" → projects grade to surface intersection
- [ ] Preview shows rubber band with grade labels
- [ ] Grade stored in vertex `grade_in`/`grade_out` and `z_source = GradeCalculated`

---

## 4.2 Offset & Projection Tools

### Stepped Offset with Grade

```rust
impl FeatureLine {
    /// Create offset feature line with optional grade
    pub fn offset_with_grade(&self, offset_dist: f64, grade: Option<Grade>, 
                              surface: Option<&Surface>) -> Result<FeatureLine> {
        // 1. Offset 2D geometry using parallel curve algorithm
        // 2. If grade provided, apply grade to offset line
        // 3. If surface provided, interpolate Z from surface
        // 4. If both grade and surface: use grade for Z, surface for validation
    }
    
    /// Create stepped offset (multiple parallel lines at intervals)
    pub fn stepped_offset(&self, offset_dist: f64, steps: usize, 
                           grade: Grade, surface: &Surface) -> Result<Vec<FeatureLine>> {
        let mut lines = Vec::new();
        let mut current = self.clone();
        
        for i in 0..steps {
            let offset = offset_dist * (i + 1) as f64;
            let mut offset_line = current.offset_with_grade(offset, Some(grade), Some(surface))?;
            offset_line.name = format!("{}_OFFSET_{}", self.name, i + 1);
            lines.push(offset_line);
            current = offset_line;  // Chain offsets
        }
        Ok(lines)
    }
    
    /// Project feature line onto surface at constant grade
    pub fn project_to_surface_at_grade(&self, grade: Grade, 
                                        surface: &Surface) -> Result<FeatureLine> {
        // Project each segment at grade until hitting surface
        // Create new feature line at intersection points
    }
}
```

### Daylight Line Generation

```rust
impl FeatureLine {
    /// Generate daylight line from feature line to surface at grade
    pub fn generate_daylight_line(&self, surface: &Surface, 
                                   cut_grade: Grade, fill_grade: Grade,
                                   max_distance: f64) -> Result<FeatureLine> {
        // For each segment:
        // 1. Get surface Z at segment endpoints
        // 2. Compare feature line Z with surface Z
        // 3. If feature line above surface (cut): project cut_grade down to surface
        // 4. If feature line below surface (fill): project fill_grade up to surface
        // 5. Collect intersection points as daylight line
    }
    
    /// Create grading surface from feature lines
    pub fn create_grading_surface(&self, lines: &[FeatureLine], 
                                   surface: &Surface) -> Result<Surface> {
        // Combine all feature lines as breaklines
        // Add boundary from surface extent
        // Triangulate with constraints
    }
}
```

### Interactive Commands

```rust
// LS_FEATURELINE_OFFSET
// LS_FEATURELINE_DAYLIGHT
// LS_FEATURELINE_PROJECT

pub struct OffsetToolCommand {
    feature_line: FeatureLine,
    offset_dist: f64,
    grade: Option<Grade>,
    surface: Option<Surface>,
    preview: Option<FeatureLine>,
}

pub struct DaylightToolCommand {
    feature_line: FeatureLine,
    surface: Surface,
    cut_grade: Grade,
    fill_grade: Grade,
    max_distance: f64,
    preview: Option<FeatureLine>,
}
```

### Acceptance Criteria
- [ ] `LS_FEATURELINE_OFFSET 5 2%` → creates parallel line at 5 units offset with 2% grade
- [ ] `LS_FEATURELINE_OFFSET 5 3 STEPS 2%` → creates 3 stepped offsets
- [ ] `LS_FEATURELINE_DAYLIGHT` → generates daylight line with cut/fill grades
- [ ] Preview shows offset/daylight line before commit
- [ ] Offset lines stored with proper Z (surface interpolated or graded)

---

## 4.3 Grading Objects

### Objective
Create grading groups (collection of feature lines defining a grading).

### Grading Object Model

```rust
// crates/landsurvey/src/featureline/grading.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GradingObject {
    pub name: String,
    pub feature_lines: Vec<String>,  // Feature line names
    pub target_surface: String,       // Existing surface name
    pub criteria: GradingCriteria,
    pub infill_surface: Option<Surface>,  // Generated grading surface
    pub style: GradingStyle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GradingCriteria {
    pub cut_grade: Grade,
    pub fill_grade: Grade,
    pub max_cut_depth: Option<f64>,
    pub max_fill_depth: Option<f64>,
    pub search_distance: f64,      // Max daylight search distance
    pub infill_method: InfillMethod,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InfillMethod {
    TIN,           // Delaunay infill
    Grid,          // Grid-based (faster, approximate)
    Hybrid,        // TIN near breaklines, grid elsewhere
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GradingStyle {
    pub cut_color: Color,
    pub fill_color: Color,
    pub contour_interval: f64,
    pub show_labels: bool,
}

impl GradingObject {
    /// Build grading surface from feature lines
    pub fn build_grading_surface(&mut self, surfaces: &HashMap<String, Surface>) 
        -> Result<Surface> {
        // 1. Collect all feature line vertices as points
        // 2. Add target surface boundary as boundary
        // 4. Add feature lines as breaklines
        // 5. Triangulate with constraints
        // 5. Return new Surface
    }
    
    /// Compute cut/fill volumes vs target surface
    pub fn compute_volumes(&self, target: &Surface) -> Result<CutFill> {
        // Use exact_composite_cut_fill or grid method
    }
    
    /// Compute daylight line for each feature line
    pub fn compute_daylight_lines(&mut self, target: &Surface) 
        -> Result<Vec<FeatureLine>> { /* ... */ }
}
```

### Interactive Command: `LS_GRADING_CREATE`

```rust
pub struct GradingCreateCommand {
    mode: GradingMode,
    name: String,
    feature_lines: Vec<String>,
    target_surface: String,
    criteria: GradingCriteria,
    preview: Option<Surface>,
}

enum GradingMode {
    SelectFeatureLines,
    SelectTargetSurface,
    EnterCriteria,
    Preview,
    Confirm,
}
```

### Acceptance Criteria
- [ ] `LS_GRADING_CREATE` → select feature lines → select target surface → enter criteria → builds grading surface
- [ ] Grading surface stored with `LANDSURVEY_SURFACE` XDATA (kind = "GRADING")
- [ ] Cut/fill volumes computed and reported
- [ ] Daylight lines generated and drawn
- [ ] Grading object saved with feature line references

---

## 4.4 Volume/Analysis

### Grading Analysis Tools

```rust
impl GradingObject {
    /// Generate slope analysis surface (color-coded by slope)
    pub fn slope_analysis(&self, surface: &Surface) -> Result<Surface> {
        // Compute slope per triangle
        // Return surface with slope as Z (or separate attribute)
    }
    
    /// Generate cut/fill depth map
    pub fn depth_map(&self, target: &Surface) -> Result<Surface> {
        // Z = depth (cut = positive, fill = negative)
    }
    
    /// Generate balance report
    pub fn balance_report(&self, target: &Surface) -> GradingReport {
        GradingReport {
            cut_volume: f64,
            fill_volume: f64,
            net_volume: f64,
            cut_area: f64,
            fill_area: f64,
            max_cut_depth: f64,
            max_fill_depth: f64,
            daylight_length: f64,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GradingReport {
    pub cut_volume: f64,
    pub fill_volume: f64,
    pub net_volume: f64,
    pub cut_area: f64,
    pub fill_area: f64,
    pub max_cut_depth: f64,
    pub max_fill_depth: f64,
    pub daylight_length: f64,
    pub balance_ratio: f64,  // cut/fill
}
```

### Interactive Reporting

```rust
// LS_GRADING_REPORT
fn grading_report(host: &mut dyn HostApi, cmd: &str) {
    let name = first_arg(cmd);
    if name.is_empty() {
        host.push_info("Usage: LS_GRADING_REPORT <grading_name>");
        return;
    }
    
    let grading = get_grading_object(&name).unwrap();
    let target = find_surface_in_document(host.document(), &grading.target_surface).unwrap();
    let report = grading.balance_report(&target.1).unwrap();
    
    host.push_output(&format!(
        "Grading Report: {}\n\
         Cut Volume: {:.2}\n\
         Fill Volume: {:.2}\n\
         Net Volume: {:.2}\n\
         Cut Area: {:.2}\n\
         Fill Area: {:.2}\n\
         Max Cut Depth: {:.2}\n\
         Max Fill Depth: {:.2}\n\
         Daylight Length: {:.2}\n\
         Balance Ratio: {:.2}",
        grading.name, report.cut_volume, report.fill_volume, report.net_volume,
        report.cut_area, report.fill_area, report.max_cut_depth, 
        report.max_fill_depth, report.daylight_length, report.balance_ratio
    ));
}
```

### Acceptance Criteria
- [ ] Create grading object from feature lines + target surface
- [ ] Generate grading surface (infill TIN)
- [ ] Compute cut/fill volumes vs target
- [ ] Generate balance report with all metrics
- [ ] Display cut/fill contours on grading surface

---

## Testing Strategy

### Unit Tests
```rust
#[test]
fn apply_grade_forward() {
    let mut fl = straight_line_100ft();
    let result = fl.apply_grade(0, Grade::from_percent(2.0), GradeDirection::Forward, None).unwrap();
    // Vertex 10 at 100ft should be 2.0 higher
    assert!((fl.vertices[10].z - (fl.vertices[0].z + 2.0)).abs() < 0.001);
}

#[test]
fn grade_to_elevation() {
    let mut fl = flat_line();
    fl.vertices[0].pt.z = 100.0;
    fl.vertices[10].pt.z = 100.0;
    let result = fl.grade_to_elevation(0, 10, 105.0, None).unwrap();
    assert!((fl.vertices[10].pt.z - 105.0).abs() < 0.001);
    // Grade should be 5% (5ft over 100ft)
    assert!((result.grade_applied.as_percent() - 5.0).abs() < 0.01);
}

#[test]
fn grade_to_surface() {
    let mut fl = flat_line();
    let surface = sloped_surface();
    let result = fl.grade_to_surface(0, Grade::from_percent(2.0), &surface, 200.0).unwrap();
    // Last vertex should be on surface
    assert!(result.vertices_affected.last().is_some());
}

#[test]
fn offset_with_grade() {
    let fl = straight_line();
    let surface = flat_surface(100.0);
    let offset = fl.offset_with_grade(10.0, Some(Grade::from_percent(2.0)), Some(&surface)).unwrap();
    // Offset line should have Z increasing at 2%
}

#[test]
fn daylight_line_generation() {
    let fl = elevated_line();
    let surface = lower_surface();
    let daylight = fl.generate_daylight_line(&surface, Grade::from_percent(3.0), Grade::from_percent(2.0), 100.0).unwrap();
    // Daylight line should intersect surface
    for v in &daylight.vertices {
        let surface_z = surface.interpolate_z(v.pt.x, v.pt.y).unwrap();
        assert!((v.pt.z - surface_z).abs() < 0.01);
    }
}

#[test]
fn grading_object_builds_surface() {
    let mut grading = GradingObject {
        feature_lines: vec!["FL1".into()],
        target_surface: "EG".into(),
        criteria: GradingCriteria {
            cut_grade: Grade::from_percent(3.0),
            fill_grade: Grade::from_percent(2.0),
            search_distance: 50.0,
            infill_method: InfillMethod::TIN,
        },
        // ...
    };
    let surface = grading.build_grading_surface(&surfaces).unwrap();
    assert!(!surface.triangles.is_empty());
}
```

### Integration Tests
```rust
#[test]
fn interactive_grade_tool() {
    // Simulate: pick vertex -> enter "2%" -> Forward -> preview -> apply
    // Verify elevations updated correctly
}

#[test]
fn grading_object_volumes_match_civil3d() {
    // Load golden file from Civil 3D
    // Compare volumes
}
```

---

## Phase 4 Deliverables

| Deliverable | File | Status |
|-------------|------|--------|
| Grade engine | `crates/landsurvey/src/featureline/grade.rs` | ☐ |
| GradeToolCommand | `crates/opencad-landsurvey-plugin/src/interactive/grade_tool.rs` | ☐ |
| Offset/Daylight tools | `crates/landsurvey/src/featureline/grade.rs` | ☐ |
| Offset/Daylight commands | `crates/opencad-landsurvey-plugin/src/interactive/offset_tool.rs` | ☐ |
| GradingObject | `crates/landsurvey/src/featureline/grading.rs` | ☐ |
| GradingCreateCommand | `crates/opencad-landsurvey-plugin/src/interactive/grading_create.rs` | ☐ |
| Volume/Analysis | `crates/landsurvey/src/featureline/grading.rs` | ☐ |
| Reporting command | `crates/opencad-landsurvey-plugin/src/dispatch.rs` | ☐ |
| Unit tests | `crates/landsurvey/src/featureline/grade.rs` tests | ☐ |
| Integration tests | `crates/opencad-landsurvey-plugin/src/dispatch.rs` tests | ☐ |

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Grade projection math complexity | Extensive unit tests with known geometries |
| Daylight intersection robustness | Robust segment-triangle intersection |
| Grading surface quality | Constrained Delaunay + quality metrics |
| Performance with many grading objects | Spatial indexing, lazy evaluation |

---

*Next: Phase 5 - Production Polish Technical Plan*