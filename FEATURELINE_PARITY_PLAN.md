# OpenCADStudio Feature Line: Civil 3D Parity Roadmap

**Version**: 1.0  
**Date**: 2026-09-24  
**Current State**: Basic feature line creation with TIN Z interpolation, XDATA storage, CSV input  
**Target**: Civil 3D `AeccDbFeatureLine` parity

---

## Executive Summary

Current implementation provides **~15% Civil 3D feature line capability**. This roadmap breaks parity into **5 phases** over ~12-18 months, prioritizing high-impact, foundational work first.

| Phase | Focus | Effort | Civil 3D Parity |
|-------|-------|--------|-----------------|
| 0 | Current | ✅ Done | 15% |
| 1 | Foundation | 2-3 months | 35% |
| 2 | Interactive Editing | 3-4 months | 55% |
| 3 | Dynamic Surface Link | 3-4 months | 75% |
| 4 | Advanced Grading | 3-4 months | 90% |
| 5 | Production Polish | 2-3 months | 100% |

---

## Phase 0: Current State (Complete)

### What Works
- [x] TIN surface discovery via `LANDSURVEY_SURFACE` XDATA
- [x] Z interpolation via `Surface::interpolate_z()`
- [x] CSV-based point input with optional Z override
- [x] LwPolyline creation on `LS-FEATURELINE` layer
- [x] Z coordinates stored in `LANDSURVEY_FEATURELINE` XDATA (CSV)
- [x] Ribbon button in "Feature Line" group
- [x] All existing tests pass (9/9)

### Known Gaps
- 2D LwPolyline only (Z in XDATA workaround)
- CSV-only input (no interactive picking)
- Static Z snapshot (no dynamic surface link)
- No editing, grading, or breakline integration

---

## Phase 1: Foundation (Months 1-3)

**Goal**: True 3D entity + interactive point picking

### 1.1 True 3D Polyline Entity
- [ ] **Add 3D polyline support to acadrust** (upstream PR or fork)
  - Extend `LwPolyline` to support `Vector3` vertices
  - Or add `Polyline3d` entity type
  - Store per-vertex Z natively
- [ ] **Migrate from XDATA CSV to native Z**
  - Remove `LANDSURVEY_FEATURELINE` Z CSV
  - Store Z in entity vertices
  - Keep XDATA for metadata (name, style, surface link)

### 1.2 Interactive Point Picking
- [ ] **Implement `InteractiveCommand` trait**
  - Define `FeatureLineCreateCommand` struct
  - Handle `on_pick(Point3d)`, `on_keyword(String)`, `on_enter()`
  - Integrate with `host.start_interactive()`
- [ ] **Point picking workflow**
  ```
  User clicks → snap to TIN → interpolate Z → preview rubber-band → 
  prompt "Accept TIN Z [123.45] or enter custom Z:" → accept → next point
  ```
- [ ] **OSNAP integration**
  - Snap to TIN vertices, edges, surface
  - Toggle TIN Z vs custom Z per vertex

### 1.3 Feature Line Data Model
- [ ] **Define `FeatureLine` struct in `landsurvey` crate**
  ```rust
  struct FeatureLine {
      name: String,
      vertices: Vec<FeatureVertex>,
      style: FeatureLineStyle,
      surface_link: Option<SurfaceLink>,  // for dynamic updates
      breakline_type: BreaklineType,
  }
  
  struct FeatureVertex {
      pt: Point3d,
      bulge: f64,
      z_source: ZSource,  // TINInterpolated | UserEntered | GradeCalculated
  }
  ```

### 1.4 Persistence & Round-trip
- [ ] **DWG/DXF round-trip via XDATA**
  - Store full feature line definition in `LANDSURVEY_FEATURELINE`
  - Handle entity cloning, copying, transformation
- [ ] **Feature line registry** (in `crate::state`)
  - Map name → `FeatureLine` for fast lookup
  - Support rename, duplicate, delete

---

## Phase 2: Interactive Editing (Months 4-7)

**Goal**: Full vertex-level editing experience

### 2.1 Elevation Editor
- [ ] **Tabular elevation grid UI**
  - Columns: Vertex #, Easting, Northing, Elevation, Z Source, Grade In, Grade Out
  - Inline editing with validation
  - Bulk operations (apply constant grade, smooth, flatten)

### 2.2 Vertex Manipulation
- [ ] **Insert vertex** - click on segment, interpolate Z from TIN
- [ ] **Delete vertex** - remove PI, maintain continuity
- [ ] **Move vertex** - drag with rubber-band, snap to TIN
- [ ] **Move vertex Z only** - vertical grip
- [ ] **Fillet/Chamfer** - add curves at PIs with bulge

### 2.3 Geometry Editing
- [ ] **Reverse direction**
- [ ] **Join feature lines** - connect end-to-end
- [ ] **Split feature line** - at vertex or distance
- [ ] **Trim/Extend** - to other geometry or surface boundary

### 2.4 Grip Editing
- [ ] **Hot grips** on vertices (move XY, move Z)
- [ ] **Segment grips** (grade, curve)
- [ ] **Multi-grip edit** - select multiple vertices

---

## Phase 3: Dynamic Surface Link (Months 8-11)

**Goal**: Civil 3D-style reactive surface linkage

### 3.1 Surface Link Architecture
- [ ] **`SurfaceLink` struct**
  ```rust
  struct SurfaceLink {
      surface_name: String,
      surface_handle: Handle,      // for fast lookup
      link_mode: LinkMode,         // Dynamic | Static | Breakline
      last_sync: DateTime,
      vertex_map: Vec<VertexLink>, // maps feature vertex → surface location
  }
  
  enum LinkMode {
      Dynamic,      // auto-update on surface change
      Static,       // snapshot at creation
      Breakline,    // feature line IS a breakline in surface
  }
  ```

### 3.2 Reactor Pattern
- [ ] **Surface change detection**
  - Monitor `LANDSURVEY_SURFACE` entities for modification
  - Use host notifications (OCS v4+ full-duplex)
- [ ] **Incremental Z update**
  - Re-interpolate only affected vertices
  - Preserve user-overridden Z (flagged as `ZSource::UserEntered`)
  - Report changes to user: "3 vertices updated from surface 'EG'"

### 3.3 Breakline Integration
- [ ] **Add feature line as surface breakline**
  - Insert vertices into surface triangulation
  - Re-triangulate affected region
  - Maintain feature line as permanent breakline
- [ ] **Surface rebuild trigger**
  - Auto-rebuild or queue for user confirmation
  - Report volume/area changes

### 3.4 Multi-Surface Support
- [ ] **Vertex-level surface assignment**
  - Different vertices can reference different surfaces
  - Useful for corridor transitions
- [ ] **Surface priority/fallback**
  - Primary surface + fallback surfaces

---

## Phase 4: Advanced Grading (Months 12-15)

**Goal**: Civil 3D grading tools parity

### 4.1 Grade/Slope Tools
- [ ] **Set grade between vertices**
  - Input: grade % or slope ratio (e.g., "2%", "3:1")
  - Apply forward/backward from selected vertex
- [ ] **Grade to surface**
  - Project from vertex to surface at specified grade
  - Find intersection, create vertex
- [ ] **Grade to elevation**
  - Set grade to hit target elevation at distance

### 4.2 Offset & Projection
- [ ] **Stepped offset**
  - Create parallel feature line at offset distance
  - Apply grade to offset (daylight slope)
- [ ] **Slope projection**
  - Project feature line onto surface at grade
  - Create daylight line automatically

### 4.3 Grading Objects
- [ ] **Feature line grading group**
  - Collection of feature lines defining a grading
  - Target surface, criteria (grade, slope, elevation)
- [ ] **Automatic surface generation**
  - Build grading surface from feature lines
  - Infill with TIN, apply as breaklines

### 4.4 Volume/Analysis
- [ ] **Cut/fill vs existing surface**
- [ ] **Material quantities**
- [ ] **Slope analysis display** (color-coded)

---

## Phase 5: Production Polish (Months 16-18)

**Goal**: Production-ready, user-friendly

### 5.1 Styles & Standards
- [ ] **Feature line styles**
  - Layer, color, linetype, lineweight per style
  - Label style (elevation, station, grade)
  - Import/export style library (XML/JSON)
- [ ] **Label engine**
  - Station/elevation labels at vertices
  - Grade labels on segments
  - Automatic placement, collision avoidance

### 5.2 Data Management
- [ ] **Feature line groups/sites**
  - Organize by site, alignment, corridor
  - Filter, isolate, batch operations
- [ ] **Import/Export**
  - LandXML feature line exchange
  - Civil 3D FLX format
  - CSV/GeoJSON for GIS

### 5.3 Performance & Scale
- [ ] **Large feature line handling** (1000+ vertices)
  - Spatial indexing for snapping
  - Lazy loading, viewport culling
- [ ] **Undo/Redo optimization**
  - Coalesced operations
  - Memory-efficient history

### 5.4 Documentation & Testing
- [ ] **User guide** with Civil 3D workflow mapping
- [ ] **Automated regression tests**
  - Golden files vs Civil 3D outputs
  - Property-based testing for geometry
- [ ] **Video tutorials** (animated SVG explainers)

---

## Technical Dependencies

### Upstream (acadrust / OCS)
| Need | Status | Workaround |
|------|--------|------------|
| 3D LwPolyline / Polyline3d | ❌ | XDATA CSV Z |
| `InteractiveCommand` trait | ✅ (OCS v2+) | CSV mode |
| Full-duplex notifications | ✅ (OCS v4+) | Polling |
| Reactor/entity events | ❌ | Manual scan |
| Custom entity types | ❌ | XDATA on LwPolyline |

### Internal (landsurvey crate)
| Need | Status |
|------|--------|
| `interpolate_z` | ✅ |
| `Surface::from_points` | ✅ |
| Delaunay with breaklines | ❌ (need constrained triangulation) |
| Exact TIN overlay | ✅ |

---

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| acadrust 3D polyline delayed | High | High | XDATA CSV + plan migration path |
| Constrained triangulation complexity | Medium | High | Use existing TIN + vertex insertion |
| OCS API instability | Low | Medium | Version pinning, feature flags |
| Performance at scale | Medium | Medium | Spatial index, lazy eval |
| Civil 3D compatibility | Medium | High | LandXML round-trip testing |

---

## Success Metrics

| Metric | Phase 1 | Phase 2 | Phase 3 | Phase 4 | Phase 5 |
|--------|---------|---------|---------|---------|---------|
| Feature line creation time | <30s | <20s | <15s | <10s | <5s |
| Vertex edit operations | N/A | <5 clicks | <3 clicks | <2 clicks | <1 click |
| Surface update latency | N/A | N/A | <2s | <1s | <500ms |
| Max vertices without lag | 100 | 500 | 2000 | 5000 | 10000+ |
| Civil 3D round-trip fidelity | 60% | 75% | 85% | 95% | 99% |

---

## Appendix: Civil 3D Feature Line Command Mapping

| Civil 3D Command | OCS Equivalent | Phase |
|------------------|----------------|-------|
| `FEATURELINE` | `LS_FEATURELINE_CREATE` | 1 |
| `FEATURELINEEDIT` | `LS_FEATURELINE_EDIT` | 2 |
| `FEATURELINEDELPI` | Vertex delete | 2 |
| `FEATURELINEADDPI` | Vertex insert | 2 |
| `FEATURELINEFLAT` | Flatten to elevation | 2 |
| `FEATURELINESETGRADE` | `LS_FEATURELINE_GRADE` | 4 |
| `FEATURELINEELEVATIONS` | Elevation editor | 2 |
| `FEATURELINEOFFSET` | `LS_FEATURELINE_OFFSET` | 4 |
| `FEATURELINEPROJ` | `LS_FEATURELINE_PROJECT` | 4 |
| `FEATURELINEBREAK` | Break at intersection | 2 |
| `FEATURELINEJOIN` | Join | 2 |
| `FEATURELINEREVERSE` | Reverse | 2 |
| `FEATURELINESTYLE` | Style manager | 5 |

---

## Appendix: File Structure (Target)

```
crates/landsurvey/src/
├── featureline/
│   ├── mod.rs              # Public API
│   ├── entity.rs           # FeatureLine, FeatureVertex
│   ├── edit.rs             # Vertex manipulation
│   ├── grade.rs            # Grade/slope calculations
│   ├── surface_link.rs     # Dynamic surface connection
│   └── breakline.rs        # Constrained triangulation
├── interactive/
│   ├── mod.rs
│   ├── featureline_create.rs  # InteractiveCommand impl
│   └── pick.rs             # Point picking utilities
crates/opencad-landsurvey-plugin/src/
├── dispatch.rs              # LS_FEATURELINE_CREATE, LS_FEATURELINE_EDIT
├── interactive.rs           # FeatureLineCreateCommand (InteractiveCommand)
├── ribbon.rs                # Feature Line group
└── state.rs                 # FeatureLine registry
```

---

*Document maintained by: Land Survey Plugin Team*  
*Review cycle: Monthly during active development*