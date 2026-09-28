//! Structured errors for the grading engine.
//!
//! Layer-C module (`std` only). Every grading routine reports failure through
//! [`GradingError`] so callers can match on the cause instead of parsing
//! strings; [`std::error::Error`] is implemented for ergonomic propagation
//! with `?` into any boxed error type.

use core::fmt;

/// Errors produced by grading geometry and daylight projection.
///
/// The variants are deliberately coarse: they classify *why* a calculation
/// failed (degeneracy, configuration, search limits) rather than carrying the
/// full geometric context, which the caller already holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GradingError {
    /// A segment was zero-length (shorter than the module's degeneracy
    /// tolerance), so no direction or arc can be derived from it.
    DegenerateSegment,
    /// The supplied vertices are collinear where a distinct corner was
    /// required (e.g. a miter or offset joint).
    CollinearPoints,
    /// A [`crate::grading::GradingCriteria`] value is out of range, non-finite,
    /// or otherwise unusable; the payload names the offending field.
    InvalidCriteria(String),
    /// A daylight ray was cast but never intersected its target (surface,
    /// elevation, or distance) inside the search window.
    RayTargetNotFound,
    /// A daylight ray travelled farther than
    /// [`crate::grading::GradingCriteria::max_projection_distance`] without
    /// reaching its target.
    RayLimitExceeded,
    /// An intermediate value overflowed, underflowed, or became non-finite
    /// (NaN/±inf) during a calculation.
    CalculationOverflow,
}

impl fmt::Display for GradingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GradingError::DegenerateSegment => {
                write!(f, "degenerate (zero-length) segment")
            }
            GradingError::CollinearPoints => {
                write!(f, "collinear points where a corner is required")
            }
            GradingError::InvalidCriteria(msg) => {
                write!(f, "invalid grading criteria: {msg}")
            }
            GradingError::RayTargetNotFound => {
                write!(f, "daylight ray did not reach its target")
            }
            GradingError::RayLimitExceeded => {
                write!(f, "daylight ray exceeded the maximum projection distance")
            }
            GradingError::CalculationOverflow => {
                write!(f, "grading calculation overflowed or became non-finite")
            }
        }
    }
}

impl std::error::Error for GradingError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_is_stable_and_non_empty() {
        assert_eq!(
            GradingError::DegenerateSegment.to_string(),
            "degenerate (zero-length) segment"
        );
        assert_eq!(
            GradingError::CollinearPoints.to_string(),
            "collinear points where a corner is required"
        );
        assert_eq!(
            GradingError::InvalidCriteria("sampling_interval".to_string()).to_string(),
            "invalid grading criteria: sampling_interval"
        );
        assert_eq!(
            GradingError::RayTargetNotFound.to_string(),
            "daylight ray did not reach its target"
        );
        assert_eq!(
            GradingError::RayLimitExceeded.to_string(),
            "daylight ray exceeded the maximum projection distance"
        );
        assert_eq!(
            GradingError::CalculationOverflow.to_string(),
            "grading calculation overflowed or became non-finite"
        );
    }

    #[test]
    fn behaves_as_std_error() {
        let err: Box<dyn std::error::Error> = Box::new(GradingError::RayLimitExceeded);
        assert!(err.source().is_none());
        // Variants stay cheap: `Clone` + `PartialEq` + `Eq` are structural.
        let clone = GradingError::DegenerateSegment.clone();
        assert_eq!(clone, GradingError::DegenerateSegment);
        assert_ne!(
            GradingError::DegenerateSegment,
            GradingError::CollinearPoints
        );
    }
}
