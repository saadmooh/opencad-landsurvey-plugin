# Phase 3: Dynamic Surface Link - Technical Implementation Plan

**Duration**: 3-4 months  
**Target Parity**: 75% Civil 3D  
**Dependencies**: Phase 2 complete (editing framework)

---

## 3.1 Surface Link Architecture

### Objective
Implement reactive link between feature lines and TIN surfaces for automatic Z updates.

### Core Architecture

```rust
// crates/landsurvey/src/featureline/surface_link.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Links a feature line to a TIN surface for dynamic Z updates
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceLink {
    pub surface_name: String,
    pub surface_handle: u64,           // For fast entity lookup
    pub link_mode: LinkMode,
    pub vertex_links: Vec<VertexLink>, // Per-vertex surface mapping
    pub last_sync: DateTime<Utc>,
    pub sync_status: SyncStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LinkMode {
    Dynamic,      // Auto-update Z on surface change
    Static,       // Snapshot at creation
    Breakline,    // Feature line IS a breakline in surface
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncStatus {
    Synced,
    OutOfDate,      // Surface changed since last sync
    Conflict,       // User override conflicts with surface
    Error(String),  // Sync failed
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VertexLink {
    pub vertex_index: usize,
    pub triangle_idx: usize,      // Containing triangle at link time
    pub barycentric: (f64, f64, f64), // Barycentric coords in triangle
    pub z_at_link: f64,           // Z at time of linking
    pub is_overridden: bool,      // User has manually overridden this Z
}

/// Manages all surface links for a feature line
pub struct SurfaceLinkManager {
    links: Vec<SurfaceLink>,
    primary_link: Option<usize>,  // Index of primary surface link
}

impl SurfaceLinkManager {
    pub fn new() -> Self { /* ... */ }
    
    /// Link to a surface (creates or updates link)
    pub fn link_surface(&mut self, fl: &mut FeatureLine, surface_name: &str, 
                        surface: &Surface, mode: LinkMode) -> Result<()> { /* ... */ }
    
    /// Unlink from a surface
    pub fn unlink_surface(&mut self, surface_name: &str) -> Result<()> { /* ... */ }
    
    /// Sync all dynamic links with current surface state
    pub fn sync_all(&mut self, fl: &mut FeatureLine, surfaces: &HashMap<String, Surface>) -> SyncReport { /* ... */ }
    
    /// Sync a single dynamic link
    fn sync_link(&mut self, fl: &mut FeatureLine, link: &mut SurfaceLink, 
                 surface: &Surface) -> VertexSyncResult { /* ... */ }
    
    /// Get primary surface for Z interpolation
    pub fn primary_surface<'a>(&self, surfaces: &'a HashMap<String, Surface>) 
        -> Option<&'a Surface> { /* ... */ }
}

/// Result of syncing one vertex
#[derive(Debug, Clone)]
pub struct VertexSyncResult {
    pub vertex_idx: usize,
    pub old_z: f64,
    pub new_z: f64,
    pub changed: bool,
    pub conflict: bool,  // User override would be overwritten
}

/// Summary of sync operation
#[derive(Debug, Default)]
pub struct SyncReport {
    pub vertices_updated: usize,
    pub vertices_conflicted: usize,
    pub vertices_unchanged: usize,
    pub errors: Vec<String>,
}
```

### Link Creation Workflow

```rust
// When creating feature line or linking later
fn link_to_surface(&mut self, surface_name: &str, surface: &Surface, mode: LinkMode) {
    let mut vertex_links = Vec::new();
    
    for (idx, vertex) in self.vertices.iter().enumerate() {
        // Find containing triangle and barycentric coords
        if let Some((tri_idx, bary)) = surface.find_containing_triangle(vertex.pt.x, vertex.pt.y) {
            vertex_links.push(VertexLink {
                vertex_index: vertex,
                triangle_idx,
                barycentric: bary,
                z_at_link: vertex.pt.z,
                is_overridden: vertex.z_source == ZSource::UserEntered,
            });
        }
    }
    
    let link = SurfaceLink {
        surface_name: surface_name.to_string(),
        surface_handle: self.get_surface_handle(surface_name),
        link_mode: mode,
        vertex_links,
        last_sync: Utc::now(),
        sync_status: SyncStatus::Synced,
    };
    
    self.surface_links.push(link);
}
```

### Acceptance Criteria
- [ ] Link feature line to TIN surface in Dynamic mode
- [ ] Vertex links store triangle + barycentric coords
- [ ] User-overridden Z marked as `is_overridden = true`
- [ ] Multiple surface links supported per feature line
- [ ] Primary surface used for new vertex interpolation

---

## 3.2 Reactor Pattern & Surface Change Detection

### Objective
Detect surface changes and trigger automatic Z updates.

### Architecture

```rust
// crates/opencad-landsurvey-plugin/src/surface_monitor.rs

use ocs_plugin_api::host::{HostApi, Notification, NotificationKind};

pub struct SurfaceMonitor {
    tracked_surfaces: HashMap<String, SurfaceSnapshot>,
    pending_notifications: Vec<SurfaceChangeEvent>,
}

#[derive(Debug, Clone)]
struct SurfaceSnapshot {
    name: String,
    handle: Handle,
    triangle_count: usize,
    node_count: usize,
    bbox: BoundingBox,
    content_hash: u64,  // Hash of all node Zs
    last_checked: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub enum SurfaceChangeEvent {
    SurfaceModified(String),
    SurfaceDeleted(String),
    SurfaceAdded(String),
}

impl SurfaceMonitor {
    pub fn new() -> Self { /* ... */ }
    
    /// Register a surface for monitoring
    pub fn track(&mut self, name: &str, handle: Handle, surface: &Surface) { /* ... */ }
    
    /// Check for changes (called periodically or on notification)
    pub fn check_changes(&mut self, host: &mut dyn HostApi) -> Vec<SurfaceChangeEvent> { /* ... */ }
    
    /// Called when host notifies of entity changes
    pub fn on_notification(&mut self, notification: &Notification) { /* ... */ }
    
    /// Get surfaces that changed since last check
    pub fn get_changed(&mut self) -> Vec<String> { /* ... */ }
    
    /// Update snapshot after successful sync
    pub fn update_snapshot(&mut self, name: &str, surface: &Surface) { /* ... */ }
}
```

### Host Integration (OCS v4+ Full-Duplex)

```rust
// In dispatch.rs - handle host notifications
fn handle_notification(host: &mut dyn HostApi, notification: &Notification) {
    match notification.kind {
        NotificationKind::EntityModified => {
            if let Some(entity) = notification.entity {
                if let Some((name, "TIN")) = surface_tag(&entity) {
                    SURFACE_MONITOR.on_notification(notification);
                }
            }
        }
        NotificationKind::EntityErased => {
            if let Some(handle) = notification.handle {
                // Check if it was a tracked surface
                SURFACE_MONITOR.on_erased(notification.handle);
            }
        }
        _ => {}
    }
}

/// Background sync task (runs periodically or on notification)
fn sync_dynamic_feature_lines(host: &mut dyn HostApi) {
    let changed = SURFACE_MONITOR.get_changed();
    for surface_name in changed {
        sync_feature_lines_for_surface(host, &surface_name);
    }
}
```

### Automatic Sync on Surface Change

```rust
fn sync_feature_lines_for_surface(host: &mut dyn HostApi, surface_name: &str) {
    // Get current surface
    let surface = find_surface_in_document(host.document(), surface_name);
    if surface.is_none() { return; }
    
    // Find all feature lines linked to this surface
    let linked_fls = find_feature_lines_linked_to(surface_name);
    
    for mut fl in linked_fls {
        let report = fl.surface_links.sync_all(&mut fl, &surfaces);
        
        if report.vertices_updated > 0 || report.vertices_conflicted > 0 {
            // Update entity geometry
            update_feature_line_entity(host, &fl);
            
            // Notify user
            host.push_info(&format!(
                "Feature line '{}': {} vertices updated from surface '{}'",
                fl.name, report.vertices_updated, surface_name
            ));
            
            if report.vertices_conflicted > 0 {
                host.push_warning(&format!(
                    "Feature line '{}': {} vertices have user overrides that would be overwritten. Use 'LS_FEATURELINE_SYNC' to resolve.",
                    fl.name, report.vertices_conflicted
                ));
            }
        }
    }
}
```

### Acceptance Criteria
- [ ] Modify TIN surface → feature line Z auto-updates
- [ ] User-overridden Z not overwritten (conflict reported)
- [ ] Surface deletion → link status = Error
- [ ] Multiple feature lines linked to same surface all update
- [ ] Sync report shows counts: updated/conflicted/unchanged
- [ ] Notification via host push_info/push_warning

---

## 3.3 Breakline Integration

### Objective
Make feature lines act as breaklines in TIN surfaces.

### Constrained Delaunay Triangulation

```rust
// crates/landsurvey/src/featureline/breakline.rs

use crate::surface::{Surface, Node, Tri};

/// Add feature line as breakline to surface
/// Modifies surface triangulation to include breakline edges
pub fn add_breakline(surface: &mut Surface, breakline: &FeatureLine) -> Result<BreaklineResult> {
    // 1. Project breakline vertices to surface XY
    // 2. Insert breakline vertices as surface nodes (if not present)
    // 2. Add breakline edges as constrained edges
    // 3. Re-triangulate affected region using constrained Delaunay
    // 4. Return modified surface or list of changes
}

#[derive(Debug, Clone)]
pub struct BreaklineResult {
    pub vertices_added: usize,
    pub triangles_modified: usize,
    pub triangles_added: usize,
    pub triangles_removed: usize,
    pub affected_area: f64,
}

/// Constrained Delaunay triangulation with breakline constraints
pub fn constrained_delaunay(
    points: &[Node],           // Existing nodes + breakline vertices
    constraints: &[[usize; 2]], // Breakline edges as node index pairs
) -> Vec<Tri> {
    // Use existing Delaunay as starting point
    // Apply edge flipping to satisfy constraints
    // Return constrained triangulation
}
```

### Breakline Integration Workflow

```rust
// In featureline.rs
impl FeatureLine {
    /// Add this feature line as breakline to linked surface
    pub fn add_as_breakline(&mut self, surface: &mut Surface) -> Result<BreaklineResult> {
        // Ensure link mode is Breakline
        for link in &mut self.surface_links {
            if link.link_mode == LinkMode::Breakline {
                let result = breakline::add_breakline(surface, self)?;
                link.sync_status = SyncStatus::Synced;
                return Ok(result);
            }
        }
        Err("No breakline link found")
    }
    
    /// Remove as breakline (restore original triangulation)
    pub fn remove_breakline(&mut self, surface: &mut Surface) -> Result<()> {
        // Re-triangulate without constraints
        // Or restore from snapshot
    }
}
```

### Surface Rebuild Trigger

```rust
// In dispatch.rs - LS_FEATURELINE_BREAKLINE command
fn featureline_breakline(host: &mut dyn HostApi, cmd: &str) {
    let args = parse_args(cmd);
    let fl_name = args.get(0).expect("feature line name");
    let surface_name = args.get(1).expect("surface name");
    let action = args.get(2).unwrap_or("add"); // "add" or "remove"
    
    let mut fl = get_feature_line(fl_name).unwrap();
    let mut surface = find_surface_in_document(host.document(), surface_name).unwrap();
    
    match action {
        "add" => {
            let result = fl.add_as_breakline(&mut surface).unwrap();
            host.push_output(&format!(
                "Added breakline: {} vertices added, {} triangles modified",
                result.vertices_added, result.triangles_modified
            ));
        }
        "remove" => {
            fl.remove_breakline(&mut surface).unwrap();
            host.push_info("Breakline removed, surface re-triangulated");
        }
        _ => host.push_error("Action must be 'add' or 'remove'"),
    }
    
    // Update surface in document
    update_surface_entity(host, &surface);
    host.bump_geometry();
    host.set_dirty();
}
```

### Acceptance Criteria
- [ ] Add feature line as breakline → surface triangulation respects feature line edges
- [ ] Breakline vertices become surface nodes
- [ ] Surface re-triangulates only affected region
- [ ] Remove breakline → surface re-triangulates without constraint
- [ ] Multiple breaklines on same surface work correctly
- [ ] Breakline Z controls surface Z along breakline

---

## 3.4 Multi-Surface Support

### Vertex-Level Surface Assignment

```rust
impl FeatureLine {
    /// Set surface for specific vertex range
    pub fn set_surface_for_range(&mut self, start: usize, end: usize, surface_name: &str) {
        // Update surface links for vertices in range
        // Split existing links if needed
    }
    
    /// Set fallback surface (used when primary has no triangle at vertex)
    pub fn set_fallback_surface(&mut self, surface_name: &str) {
        // Store as fallback in surface links
    }
    
    /// Get interpolated Z using surface priority
    pub fn interpolate_z(&self, idx: usize, surfaces: &HashMap<String, Surface>) -> Option<f64> {
        // Try primary surface
        // If no triangle contains vertex, try fallback
        // Return None if no surface covers point
    }
}
```

### Acceptance Criteria
- [ ] Different vertices can reference different surfaces
- [ ] Fallback surface used when primary has no coverage
- [ ] Priority order: primary → fallback1 → fallback2
- [ ] Vertex Z interpolated from correct surface

---

## Testing Strategy

### Unit Tests
```rust
#[test]
fn surface_link_creation() {
    let mut fl = feature_line();
    let surface = test_surface();
    
    fl.link_surface("EG", &surface, LinkMode::Dynamic).unwrap();
    
    assert_eq!(fl.surface_links.len(), 1);
    assert_eq!(fl.surface_links[0].link_mode, LinkMode::Dynamic);
    assert_eq!(fl.surface_links[0].vertex_links.len(), fl.vertices.len());
}

#[test]
fn dynamic_sync_updates_z() {
    let mut fl = feature_line();
    let mut surface = test_surface();
    fl.link_surface("EG", &surface, LinkMode::Dynamic).unwrap();
    
    // Modify surface Z
    surface.nodes[0][2] += 5.0;
    
    let report = fl.surface_links.sync_all(&mut fl, &surfaces);
    assert_eq!(report.vertices_updated, fl.vertices.len());
    // All Z updated
}

#[test]
fn user_override_not_overwritten() {
    let mut fl = feature_line();
    fl.vertices[0].z_source = ZSource::UserEntered;
    fl.vertices[0].pt.z = 150.0;  // User override
    
    let surface = test_surface_modified(); // Different Z at vertex 0
    fl.link_surface("EG", &surface, LinkMode::Dynamic).unwrap();
    
    let report = fl.sync_all(&surfaces);
    // Vertex 0 should be conflicted, not updated
    assert!(report.vertices_conflicted > 0);
}

#[test]
fn breakline_addition_respects_edges() {
    let mut surface = test_surface();
    let fl = straight_feature_line();
    
    let result = add_breakline(&mut surface, &fl).unwrap();
    // Breakline edges should be in triangulation
    assert!(result.triangles_modified > 0);
}
```

### Integration Tests
```rust
#[test]
fn dynamic_surface_update_triggers_feature_line_sync() {
    // 1. Create surface, feature line linked Dynamic
    // 2. Modify surface entity in document
    // 3. Trigger notification
    // 4. Verify feature line Z updated
}

#[test]
fn breakline_addition_modifies_surface() {
    // 1. Create surface + feature line
    // 2. Add as breakline
    // 3. Verify surface triangulation includes breakline edges
}
```

---

## Phase 3 Deliverables

| Deliverable | File | Status |
|-------------|------|--------|
| SurfaceLink types | `crates/landsurvey/src/featureline/surface_link.rs` | ☐ |
| SurfaceLinkManager | `crates/landsurvey/src/featureline/surface_link.rs` | ☐ |
| SurfaceMonitor | `crates/opencad-landsurvey-plugin/src/surface_monitor.rs` | ☐ |
| Notification handling | `crates/opencad-landsurvey-plugin/src/dispatch.rs` | ☐ |
| Breakline integration | `crates/landsurvey/src/featureline/breakline.rs` | ☐ |
| Constrained Delaunay | `crates/landsurvey/src/featureline/breakline.rs` | ☐ |
| LS_FEATURELINE_BREAKLINE command | `crates/opencad-landsurvey-plugin/src/dispatch.rs` | ☐ |
| Multi-surface support | `crates/landsurvey/src/featureline/surface_link.rs` | ☐ |
| Unit tests | `crates/landsurvey/src/featureline/surface_link.rs` tests | ☐ |
| Integration tests | `crates/opencad-landsurvey-plugin/src/dispatch.rs` tests | ☐ |

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Constrained triangulation complexity | Use incremental edge flipping; fallback to full rebuild |
| OCS notification reliability | Polling fallback (every 2s) + notification |
| Large surface sync performance | Incremental sync (only changed triangles) |
| Breakline insertion performance | Local re-triangulation (Bowyer-Watson) |

---

*Next: Phase 4 - Advanced Grading Technical Plan*