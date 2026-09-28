//! Interactive commands for Land Survey plugin.
//!
//! This module implements `InteractiveCommand` traits for interactive point picking
//! and feature line creation.

use acadrust::{EntityType, LwPolyline, Vector2};
use landsurvey::featureline::{FeatureLine, FeatureVertex, Point3d, ZSource};
use landsurvey::surface::Surface;
use ocs_plugin_api::host::{CommandStep, InteractiveCommand};

/// State of the feature line creation interactive command.
#[derive(Debug, Clone, PartialEq)]
enum CreateState {
    /// Waiting for the first point.
    PickingFirstPoint,
    /// Waiting for subsequent points.
    PickingNextPoint,
    /// Waiting for user to confirm or override TIN Z.
    ConfirmingZ { tin_z: f64 },
    /// Feature line creation finished.
    Finished,
}

/// Interactive command for creating a feature line by picking points on a TIN surface.
pub struct FeatureLineCreateCommand {
    surface_name: String,
    surface: Surface,
    feature_line: FeatureLine,
    state: CreateState,
}

impl FeatureLineCreateCommand {
    pub fn new(surface_name: String, surface: Surface) -> Self {
        Self {
            surface_name,
            surface,
            feature_line: FeatureLine::new("FL".to_string()),
            state: CreateState::PickingFirstPoint,
        }
    }

    /// Interpolate Z from TIN surface at given XY.
    fn interpolate_z(&self, x: f64, y: f64) -> Option<f64> {
        self.surface.interpolate_z(x, y)
    }

    /// Add a vertex to the feature line.
    fn add_vertex(&mut self, x: f64, y: f64, z: f64, z_source: ZSource) {
        self.feature_line
            .push_vertex(FeatureVertex::new(Point3d::new(x, y, z), 0.0, z_source));
    }

    /// Handle keyword input encoded as special point values.
    /// Keywords are encoded as [NaN, keyword_code, 0.0] where keyword_code:
    /// 1.0 = UNDO, 2.0 = CLOSE, 3.0 = CANCEL
    fn handle_keyword(&mut self, code: f64) -> CommandStep {
        match code {
            1.0 => {
                // UNDO
                if self.feature_line.vertex_count() > 1 {
                    self.feature_line
                        .remove_vertex(self.feature_line.vertex_count() - 1);
                }
                self.state = CreateState::PickingNextPoint;
            }
            2.0 => {
                // CLOSE
                if self.feature_line.vertex_count() >= 3 {
                    self.state = CreateState::Finished;
                }
            }
            3.0 => {
                // CANCEL
                return CommandStep::Cancel;
            }
            _ => {}
        }
        CommandStep::NeedPoint
    }

    /// Build the LwPolyline entity from the feature line data.
    fn build_lwpolyline(&self) -> LwPolyline {
        let mut lw = LwPolyline::new();
        for vertex in &self.feature_line.vertices {
            lw.add_point_with_bulge(Vector2::new(vertex.pt.x, vertex.pt.y), vertex.bulge);
        }
        lw.is_closed = self.feature_line.is_closed();
        lw
    }
}

impl InteractiveCommand for FeatureLineCreateCommand {
    fn prompt(&self) -> String {
        match &self.state {
            CreateState::PickingFirstPoint => {
                format!("Pick first point on surface '{}': ", self.surface_name)
            }
            CreateState::PickingNextPoint => {
                format!(
                    "Pick next point on surface '{}' [Undo/Close/Cancel]: ",
                    self.surface_name
                )
            }
            CreateState::ConfirmingZ { tin_z } => {
                format!("TIN Z = {:.3}. Accept [Enter] or enter custom Z: ", tin_z)
            }
            CreateState::Finished => "Feature line created.".to_string(),
        }
    }

    fn on_point(&mut self, point: [f64; 3]) -> CommandStep {
        let x = point[0];
        let y = point[1];

        // Check for keyword encoding: [NaN, keyword_code, 0.0]
        if x.is_nan() {
            let code = y;
            return self.handle_keyword(code);
        }

        let x = point[0];
        let y = point[1];

        // Interpolate Z from TIN
        let tin_z = match self.interpolate_z(x, y) {
            Some(z) => z,
            None => {
                return CommandStep::NeedPoint;
            }
        };

        match self.state {
            CreateState::PickingFirstPoint => {
                // First point - add directly with TIN Z
                self.add_vertex(x, y, tin_z, ZSource::TINInterpolated);
                self.state = CreateState::PickingNextPoint;
            }
            CreateState::PickingNextPoint => {
                // Subsequent point - ask for Z confirmation
                self.state = CreateState::ConfirmingZ { tin_z };
            }
            CreateState::ConfirmingZ { tin_z } => {
                // User picked a point while in confirmation state - add with TIN Z
                self.add_vertex(x, y, tin_z, ZSource::TINInterpolated);
                self.state = CreateState::PickingNextPoint;
            }
            CreateState::Finished => {
                // Already finished
            }
        }
        CommandStep::NeedPoint
    }

    fn on_enter(&mut self) -> CommandStep {
        match &self.state {
            CreateState::ConfirmingZ { .. } => {
                // Accept TIN Z - the vertex was already added
                self.state = CreateState::PickingNextPoint;
            }
            CreateState::Finished => {
                // Entity completion lives here: the host has no `on_finish`
                // hook, so the Finished state commits the polyline and ends
                // the command. (Breakline application + XDATA tagging happen
                // in the non-interactive `featureline_create_from_csv` path,
                // which owns a `&mut dyn HostApi`.)
                let lw = self.build_lwpolyline();
                let entity = EntityType::LwPolyline(lw);
                return CommandStep::CommitAndEnd(entity);
            }
            _ => {}
        }
        CommandStep::NeedPoint
    }
}
