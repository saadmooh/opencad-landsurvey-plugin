//! Grading criteria: slope units, daylight targets, and validated defaults.
//!
//! Layer-C module (`std` only). Values here are *intent* — how a designer
//! expresses a slope and which surface a daylight line should seek — while the
//! heavy lifting stays in [`super::math`].

use core::fmt;

use crate::grading::GradingError;

/// A slope expressed as a rise/run value.
///
/// Grading proposals typically reach us as ratios (`2 : 1`) or percentages
/// (`25%`); both normalize to the same dimensionless run-per-rise gradient so
/// downstream maths never branches on the source unit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SlopeValue {
    /// Rise over run as an exact pair, e.g. `Ratio { h: 2.0, v: 1.0 }` for a
    /// `2 : 1` cut slope (two units horizontal per one unit vertical).
    Ratio {
        /// Horizontal run component.
        h: f64,
        /// Vertical rise component.
        v: f64,
    },
    /// Slope as a percentage, e.g. `Percent(25.0)` for `25%`.
    ///
    /// Stored as the numeric percentage itself, not its fraction (`0.25`).
    Percent(f64),
}

impl SlopeValue {
    /// Dimensionless gradient `run / rise` used by the daylight equations.
    ///
    /// * `Ratio { h, v }` → `h / v`, or `0.0` when `|h|` is below `1e-9`
    ///   (a vertical face: no usable run-per-rise value).
    /// * `Percent(p)` → `p / 100.0`, so `25%` yields `0.25`.
    pub fn as_gradient(self) -> f64 {
        match self {
            SlopeValue::Ratio { h, v } => {
                if h.abs() < 1e-9 {
                    0.0
                } else {
                    h / v
                }
            }
            SlopeValue::Percent(p) => p / 100.0,
        }
    }
}

/// Where a daylight (projection) line terminates once it leaves the grading
/// site.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum GradingTarget {
    /// Intersect the existing ground surface triangle mesh.
    #[default]
    Surface,
    /// Intersect a constant elevation plane (`z = value`).
    Elevation,
    /// Stop after a fixed horizontal distance.
    Distance,
    /// Stop at an arbitrary 3D point; the ray is solved toward it.
    Point,
}

/// Full criteria set controlling one grading run.
///
/// Every field is validated together by [`GradingCriteria::validate`]; the
/// [`Default`] implementation is a deliberately conservative starting point for
/// site grading (surface daylighting, `2:1` cut, `3:1` fill, fine sagitta
/// tolerance).
#[derive(Debug, Clone, PartialEq)]
pub struct GradingCriteria {
    /// Which entity terminates a daylight projection.
    pub target: GradingTarget,
    /// Slope applied to cut regions (excavation faces).
    pub cut_slope: SlopeValue,
    /// Slope applied to fill regions (embankment faces).
    pub fill_slope: SlopeValue,
    /// Horizontal sampling interval, in world units (meters), between
    /// evaluated stations along a feature line. Must be `> 0` and finite.
    pub sampling_interval: f64,
    /// Maximum allowed chord-to-arc sagitta when densifying bulge arcs, in
    /// world units. Must be finite; `<= 0` selects the fixed-step fallback in
    /// [`super::math::densify_bulge`].
    pub max_sagitta: f64,
    /// Hard cap on daylight projection length, in world units. Exceeding it
    /// raises [`GradingError::RayLimitExceeded`]. Must be positive and finite.
    pub max_projection_distance: f64,
}

impl Default for GradingCriteria {
    /// Surface daylighting, `2:1` cut, `3:1` fill, 1 m stations, 1 cm sagitta,
    /// 500 m projection cap.
    fn default() -> Self {
        GradingCriteria {
            target: GradingTarget::Surface,
            cut_slope: SlopeValue::Ratio { h: 2.0, v: 1.0 },
            fill_slope: SlopeValue::Ratio { h: 3.0, v: 1.0 },
            sampling_interval: 1.0,
            max_sagitta: 0.01,
            max_projection_distance: 500.0,
        }
    }
}

impl GradingCriteria {
    /// Check every field for finiteness and range, returning the first
    /// offending field wrapped in [`GradingError::InvalidCriteria`].
    ///
    /// Rules:
    /// * `sampling_interval` must be finite and `> 0`.
    /// * `max_sagitta` must be finite (zero or negative selects the fallback
    ///   step, so it is allowed).
    /// * `max_projection_distance` must be finite and `> 0`.
    /// * both slopes must be finite pairs/scalars.
    pub fn validate(&self) -> Result<(), GradingError> {
        if !self.sampling_interval.is_finite() || self.sampling_interval <= 0.0 {
            return Err(GradingError::InvalidCriteria(
                "sampling_interval must be finite and greater than 0".to_string(),
            ));
        }
        if !self.max_sagitta.is_finite() {
            return Err(GradingError::InvalidCriteria(
                "max_sagitta must be finite".to_string(),
            ));
        }
        if !self.max_projection_distance.is_finite() || self.max_projection_distance <= 0.0 {
            return Err(GradingError::InvalidCriteria(
                "max_projection_distance must be finite and greater than 0".to_string(),
            ));
        }
        for (name, slope) in [
            ("cut_slope", self.cut_slope),
            ("fill_slope", self.fill_slope),
        ] {
            let finite = match slope {
                SlopeValue::Ratio { h, v } => h.is_finite() && v.is_finite(),
                SlopeValue::Percent(p) => p.is_finite(),
            };
            if !finite {
                return Err(GradingError::InvalidCriteria(format!(
                    "{name} must be finite"
                )));
            }
        }
        Ok(())
    }
}

impl fmt::Display for GradingCriteria {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "GradingCriteria {{ target: {:?}, cut: {:?}, fill: {:?}, \
             sampling_interval: {}, max_sagitta: {}, max_projection_distance: {} }}",
            self.target,
            self.cut_slope,
            self.fill_slope,
            self.sampling_interval,
            self.max_sagitta,
            self.max_projection_distance
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slope_value_gradients() {
        // 2:1 → run per rise = 2.0
        assert_eq!(SlopeValue::Ratio { h: 2.0, v: 1.0 }.as_gradient(), 2.0);
        // 3:1 → 3.0
        assert_eq!(SlopeValue::Ratio { h: 3.0, v: 1.0 }.as_gradient(), 3.0);
        // Vertical face (vanishing run) → 0.0, never NaN.
        assert_eq!(SlopeValue::Ratio { h: 0.0, v: 1.0 }.as_gradient(), 0.0);
        // 25% → 0.25 exactly.
        assert_eq!(SlopeValue::Percent(25.0).as_gradient(), 0.25);
        assert_eq!(SlopeValue::Percent(100.0).as_gradient(), 1.0);
        assert_eq!(SlopeValue::Percent(0.0).as_gradient(), 0.0);
    }

    #[test]
    fn default_criteria_values() {
        let c = GradingCriteria::default();
        assert_eq!(c.target, GradingTarget::Surface);
        assert_eq!(c.cut_slope, SlopeValue::Ratio { h: 2.0, v: 1.0 });
        assert_eq!(c.fill_slope, SlopeValue::Ratio { h: 3.0, v: 1.0 });
        assert_eq!(c.sampling_interval, 1.0);
        assert_eq!(c.max_sagitta, 0.01);
        assert_eq!(c.max_projection_distance, 500.0);
        assert_eq!(c.validate(), Ok(()));
    }

    #[test]
    fn validate_rejects_bad_fields() {
        let mut c = GradingCriteria::default();
        c.sampling_interval = 0.0;
        assert!(matches!(
            c.validate(),
            Err(GradingError::InvalidCriteria(ref m)) if m.contains("sampling_interval")
        ));

        let mut c = GradingCriteria::default();
        c.max_sagitta = f64::NAN;
        assert!(matches!(
            c.validate(),
            Err(GradingError::InvalidCriteria(ref m)) if m.contains("max_sagitta")
        ));

        let mut c = GradingCriteria::default();
        c.max_projection_distance = -1.0;
        assert!(matches!(
            c.validate(),
            Err(GradingError::InvalidCriteria(ref m)) if m.contains("max_projection_distance")
        ));

        let mut c = GradingCriteria::default();
        c.fill_slope = SlopeValue::Percent(f64::INFINITY);
        assert!(matches!(
            c.validate(),
            Err(GradingError::InvalidCriteria(ref m)) if m.contains("fill_slope")
        ));
    }

    #[test]
    fn display_mentions_every_field() {
        let s = GradingCriteria::default().to_string();
        for needle in [
            "target",
            "cut",
            "fill",
            "sampling_interval",
            "max_sagitta",
            "max_projection_distance",
        ] {
            assert!(s.contains(needle), "missing {needle} in {s}");
        }
    }
}
