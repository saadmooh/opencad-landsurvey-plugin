# Feature Line Parity Plan - Detailed Technical Plans

This folder contains detailed technical implementation plans for each phase of the Civil 3D Feature Line parity roadmap.

## Folder Structure

```
FEATURELINE_PARITY_PLAN/
├── README.md                           # This file
├── PHASE1_Foundation_TechPlan.md       # Phase 1: Foundation (Months 1-3)
├── PHASE2_InteractiveEditing_TechPlan.md  # Phase 2: Interactive Editing (Months 4-7)
├── PHASE3_DynamicSurfaceLink_TechPlan.md  # Phase 3: Dynamic Surface Link (Months 8-11)
├── PHASE4_AdvancedGrading_TechPlan.md     # Phase 4: Advanced Grading (Months 12-15)
└── PHASE5_ProductionPolish_TechPlan.md    # Phase 5: Production Polish (Months 16-18)
```

## Master Plan Reference

See `../FEATURELINE_PARITY_PLAN.md` for the high-level roadmap overview, risk assessment, success metrics, and Civil 3D command mapping.

---

## Phase Overview

| Phase | Folder File | Duration | Target Parity | Key Deliverables |
|-------|-------------|----------|---------------|------------------|
| 1 | `PHASE1_Foundation_TechPlan.md` | 2-3 months | 35% | 3D Polyline, InteractiveCommand, FeatureLine entity, XDATA persistence |
| 2 | `PHASE2_InteractiveEditing_TechPlan.md` | 3-4 months | 55% | Elevation editor, vertex manipulation, grips, join/split/trim |
| 3 | `PHASE3_DynamicSurfaceLink_TechPlan.md` | 3-4 months | 75% | SurfaceLink, Reactor pattern, Breakline integration, Multi-surface |
| 4 | `PHASE4_AdvancedGrading_TechPlan.md` | 3-4 months | 90% | Grade tools, Offset/Daylight, Grading objects, Volume analysis |
| 5 | `PHASE5_ProductionPolish_TechPlan.md` | 2-3 months | 100% | Styles, Labels, Groups, Import/Export, Performance, Documentation |

---

## Cross-Phase Dependencies

```
Phase 1 (Foundation)
    │
    ├── 3D Polyline entity (acadrust)
    ├── InteractiveCommand framework
    ├── FeatureLine core types
    └── XDATA persistence
            │
            ▼
Phase 2 (Interactive Editing)
    │
    ├── Elevation Editor (tabular)
    ├── Vertex manipulation (insert/delete/move/fillet)
    ├── Grip editing
    └── Join/Split/Trim/Extend
            │
            ▼
Phase 3 (Dynamic Surface Link)
    │
    ├── SurfaceLink architecture
    ├── Reactor pattern (OCS notifications)
    ├── Breakline integration (constrained Delaunay)
    └── Multi-surface support
            │
            ▼
Phase 4 (Advanced Grading)
    │
    ├── Grade/Slope engine
    ├── Offset/Daylight/Projection tools
    ├── Grading Objects (groups)
    └── Volume/Analysis reporting
            │
            ▼
Phase 5 (Production Polish)
    │
    ├── Style system & Label engine
    ├── Group/Site management
    ├── Import/Export (LandXML, FLX, GeoJSON)
    ├── Spatial indexing & Performance
    ├── Documentation & Golden tests
    └── Civil 3D round-trip validation
```

---

## Key Technical Decisions

### 1. 3D Polyline Strategy
- **Primary**: Upstream acadrust `Polyline3d` support
- **Fallback**: XDATA CSV Z storage on 2D `LwPolyline`
- **Migration**: Runtime detection, automatic upgrade on save

### 2. Interactive Command Framework
- Use OCS `InteractiveCommand` trait (v2+)
- Rubber band visualization during picking
- OSNAP integration with TIN priority
- Keyword-driven workflow (Civil 3D-like)

### 3. Surface Link Architecture
- **LinkMode**: Dynamic | Static | Breakline
- **VertexLink**: Triangle + barycentric coords for stable mapping
- **SyncStatus**: Synced | OutOfDate | Conflict | Error
- **Reactor**: OCS v4+ full-duplex + polling fallback

### 4. Breakline Integration
- Constrained Delaunay via incremental edge flipping
- Local re-triangulation (Bowyer-Watson) for performance
- Breakline vertices become surface nodes

### 5. Grade Engine
- Multiple formats: %, Ratio, Degrees, PerMille
- Direction: Forward | Backward | Both
- Targets: Vertex, Elevation, Surface intersection
- Preview with rubber band + grade labels

### 6. Data Persistence
- **Primary**: `LANDSURVEY_FEATURELINE` XDATA (JSON)
- **Round-trip**: DWG/DXF via XDATA, LandXML/FLX for exchange
- **Registry**: In-memory `HashMap<String, FeatureLine>` with handle mapping

---

## Testing Strategy Summary

| Test Type | Scope | Location |
|-----------|-------|----------|
| Unit | Core algorithms (grade, interpolate, triangulation) | `landsurvey/src/featureline/` |
| Integration | Command dispatch, XDATA round-trip | `plugin/src/dispatch.rs` |
| Golden | Civil 3D FLX round-trip | `landsurvey/tests/golden/` |
| Performance | 10k vertices benchmarks | `landsurvey/benches/` |
| Civil 3D | Volume/geometry accuracy | `landsurvey/tests/civil3d/` |

---

## Getting Started

1. **Phase 1**: Start with `PHASE1_Foundation_TechPlan.md`
   - Implement 3D polyline in acadrust
   - Build FeatureLine entity in `landsurvey`
   - Implement InteractiveCommand for point picking

2. **Track Progress**: Each phase file has checkboxes `[ ]` for tasks

3. **Run Tests**: `cargo test -p landsurvey -p opencad-landsurvey-plugin`

4. **Benchmarks**: `cargo bench -p landsurvey -- featureline`

---

## Contributing

1. Pick a task from the phase checklists
2. Implement with unit tests
3. Add integration test in `dispatch.rs`
4. Update Civil 3D command mapping table
5. Run full test suite before PR

---

*Generated: 2026-09-24*  
*Part of OpenCADStudio Land Survey Plugin*