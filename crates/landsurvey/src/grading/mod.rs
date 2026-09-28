//! Grading engine for the land-survey kernel.
//!
//! Layer-C module (`std` only — no host/CAD types). Phase 1 (foundations) is
//! in place:
//!
//! * [`error`] — structured failure modes ([`GradingError`]).
//! * [`criteria`] — slope units, daylight targets, validated defaults
//!   ([`SlopeValue`], [`GradingTarget`], [`GradingCriteria`]).
//! * [`math`] — bulge densification, outward normals, vertex bisectors, and
//!   convexity tests.
//! * [`daylight`] — slope projection from a boundary station onto existing
//!   ground ([`daylight`], [`DaylightKind`], [`DaylightResult`]).
//! * [`project`] — feature line sampling and densification for grading.
//! * [`infill`] — grading surface and infill generation (Phase 3).
//
//! Higher-level grading workflows (feature-line projection, corners, infill,
//! paste) will be added here as subsequent phases land.

pub mod criteria;
pub mod daylight;
pub mod error;
pub mod infill;
pub mod math;
pub mod project;

pub use criteria::{GradingCriteria, GradingTarget, SlopeValue};
pub use daylight::{daylight, DaylightKind, DaylightResult};
pub use error::GradingError;
pub use infill::{create_grading_surface, mesh_grading_infill, mesh_grading_slopes};
pub use math::{densify_bulge, is_convex_corner, orient2d, outward_normal_2d, vertex_bisector_2d};

