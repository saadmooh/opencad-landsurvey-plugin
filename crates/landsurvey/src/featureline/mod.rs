//! Feature Line module for landsurvey engine.
//!
//! This module provides 3D feature line functionality with elevation data,
//! surface linking, grading, and Civil 3D compatibility.

pub mod breakline;
pub mod entity;

pub use entity::{
    BreaklineResult, BreaklineType, EntityColor, FeatureLine, FeatureLineStyle,
    FeatureLineSyncReport, FeatureLineSyncResult, FeatureVertex, GradeFormat, LineWeight, LinkMode,
    Point3d, SurfaceLink, SyncReport, SyncStatus, VertexLink, VertexMarker, VertexSyncResult,
    ZSource,
};
