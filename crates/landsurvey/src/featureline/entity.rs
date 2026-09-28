//! Feature Line core types for landsurvey engine.
//!
//! This module defines the data structures for 3D feature lines with elevation
//! data, surface linking, and grading capabilities.

use serde::{Deserialize, Serialize};

/// A 3D point with optional bulge for curved segments.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point3d {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Point3d {
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    pub fn xy(&self) -> (f64, f64) {
        (self.x, self.y)
    }
}

/// Source of Z elevation for a vertex.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ZSource {
    /// Z interpolated from TIN surface at creation time.
    TINInterpolated,
    /// Z manually entered by user.
    UserEntered,
    /// Z calculated from grade/slope tool.
    GradeCalculated,
    /// Z projected from breakline/surface intersection.
    BreaklineProjected,
}

/// A single vertex in a feature line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeatureVertex {
    /// 3D position (Easting, Northing, Elevation).
    pub pt: Point3d,
    /// Bulge for curved segment starting at this vertex (tan(sweep/4)).
    /// Positive = CCW arc.
    pub bulge: f64,
    /// How the Z elevation was determined.
    pub z_source: ZSource,
    /// Grade entering this vertex from previous (percent).
    pub grade_in: Option<f64>,
    /// Grade leaving this vertex to next (percent).
    pub grade_out: Option<f64>,
}

impl FeatureVertex {
    pub fn new(pt: Point3d, bulge: f64, z_source: ZSource) -> Self {
        Self {
            pt,
            bulge,
            z_source,
            grade_in: None,
            grade_out: None,
        }
    }

    pub fn new_2d(x: f64, y: f64, z: f64) -> Self {
        Self::new(Point3d::new(x, y, z), 0.0, ZSource::TINInterpolated)
    }
}

/// Link mode between feature line and surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LinkMode {
    /// Auto-update Z when surface changes.
    Dynamic,
    /// Z fixed at creation time (snapshot).
    Static,
    /// Feature line acts as breakline in surface triangulation.
    Breakline,
}

/// Synchronization status for a surface link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncStatus {
    /// Z values match surface.
    Synced,
    /// Surface has changed since last sync.
    OutOfDate,
    /// User override would be overwritten by surface.
    Conflict,
    /// Sync failed with error message.
    Error(String),
}

/// Per-vertex link to a surface triangle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VertexLink {
    /// Index of vertex in feature line.
    pub vertex_index: usize,
    /// Index of containing triangle in surface at link time.
    pub triangle_idx: usize,
    /// Barycentric coordinates (u, v, w) in the triangle.
    pub barycentric: (f64, f64, f64),
    /// Z value at time of linking.
    pub z_at_link: f64,
    /// True if user has manually overridden this Z.
    pub is_overridden: bool,
}

/// Link between feature line and a TIN surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceLink {
    /// Name of the linked surface.
    pub surface_name: String,
    /// Entity handle of the surface for fast lookup.
    pub surface_handle: u64,
    /// Link mode (Dynamic, Static, Breakline).
    pub link_mode: LinkMode,
    /// Per-vertex surface mapping.
    pub vertex_links: Vec<VertexLink>,
    /// Timestamp of last successful sync.
    pub last_sync: chrono::DateTime<chrono::Utc>,
    /// Current synchronization status.
    pub sync_status: SyncStatus,
}

impl SurfaceLink {
    pub fn new(surface_name: String, surface_handle: u64, mode: LinkMode) -> Self {
        Self {
            surface_name,
            surface_handle,
            link_mode: mode,
            vertex_links: Vec::new(),
            last_sync: chrono::Utc::now(),
            sync_status: SyncStatus::Synced,
        }
    }
}

/// Synchronization result for a single vertex.
#[derive(Debug, Clone, PartialEq)]
pub struct VertexSyncResult {
    pub vertex_idx: usize,
    pub old_z: f64,
    pub new_z: f64,
    pub changed: bool,
    pub conflict: bool,
}

/// Summary of a sync operation.
#[derive(Debug, Default, Clone)]
pub struct SyncReport {
    pub vertices_updated: usize,
    pub vertices_conflicted: usize,
    pub vertices_unchanged: usize,
    pub errors: Vec<String>,
}

/// Feature line style definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeatureLineStyle {
    pub name: String,
    pub description: String,
    pub layer: String,
    pub color: EntityColor,
    pub linetype: String,
    pub lineweight: LineWeight,
    pub show_vertices: bool,
    pub vertex_marker: VertexMarker,
    pub vertex_size: f64,
    pub show_grades: bool,
    pub grade_precision: u8,
    pub grade_format: GradeFormat,
    pub show_elevations: bool,
    pub elevation_precision: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VertexMarker {
    None,
    Square,
    Circle,
    Diamond,
    Cross,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GradeFormat {
    Percent,
    Ratio,
    Degrees,
    PerMille,
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
            show_vertices: true,
            vertex_marker: VertexMarker::Square,
            vertex_size: 1.0,
            show_grades: true,
            grade_precision: 2,
            grade_format: GradeFormat::Percent,
            show_elevations: true,
            elevation_precision: 2,
        }
    }
}

/// Color representation matching acadrust.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityColor {
    ByLayer,
    ByBlock,
    TrueColor(u8, u8, u8),
    Index(u16),
}

/// Line weight matching acadrust.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineWeight {
    ByLayer,
    ByBlock,
    Default,
    W000,
    W005,
    W009,
    W013,
    W015,
    W018,
    W020,
    W025,
    W030,
    W035,
    W040,
    W050,
    W053,
    W060,
    W070,
    W080,
    W090,
    W100,
    W106,
    W120,
    W140,
    W158,
    W200,
    W211,
}

/// Breakline type for surface integration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BreaklineType {
    Standard,
    Wall,
    RetainingWall,
    Curb,
    Gutter,
    FlowLine,
}

/// Result of adding a breakline to a surface.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BreaklineResult {
    pub vertices_added: usize,
    pub triangles_before: usize,
    pub triangles_modified: usize,
    pub triangles_added: usize,
    pub triangles_removed: usize,
    pub affected_area: f64,
}

/// Sync report for feature line surface synchronization.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FeatureLineSyncReport {
    pub vertices_updated: usize,
    pub vertices_conflicted: usize,
    pub vertices_unchanged: usize,
    pub errors: Vec<String>,
}

/// Sync result for a feature line.
#[derive(Debug, Clone, Default)]
pub struct FeatureLineSyncResult {
    pub vertices_updated: usize,
    pub vertices_conflicted: usize,
    pub vertices_unchanged: usize,
    pub errors: Vec<String>,
}

/// A 3D feature line with elevation data, surface linking, and grading support.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeatureLine {
    pub name: String,
    pub description: String,
    pub vertices: Vec<FeatureVertex>,
    pub style: FeatureLineStyle,
    pub surface_links: Vec<SurfaceLink>,
    pub breakline_type: BreaklineType,
    pub created: chrono::DateTime<chrono::Utc>,
    pub modified: chrono::DateTime<chrono::Utc>,
}

impl FeatureLine {
    pub fn new(name: String) -> Self {
        let now = chrono::Utc::now();
        Self {
            name,
            description: String::new(),
            vertices: Vec::new(),
            style: FeatureLineStyle::default(),
            surface_links: Vec::new(),
            breakline_type: BreaklineType::Standard,
            created: now,
            modified: now,
        }
    }

    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    pub fn is_closed(&self) -> bool {
        if self.vertices.len() < 2 {
            return false;
        }
        let first = &self.vertices[0].pt;
        let last = &self.vertices[self.vertices.len() - 1].pt;
        const EPS: f64 = 1e-9;
        (first.x - last.x).abs() < EPS && (first.y - last.y).abs() < EPS
    }

    /// Get primary surface link (first Dynamic link).
    pub fn primary_surface_link(&self) -> Option<&SurfaceLink> {
        self.surface_links
            .iter()
            .find(|l| l.link_mode == LinkMode::Dynamic)
    }

    /// Get primary surface link mutably.
    pub fn primary_surface_link_mut(&mut self) -> Option<&mut SurfaceLink> {
        self.surface_links
            .iter_mut()
            .find(|l| l.link_mode == LinkMode::Dynamic)
    }

    /// Add a vertex at the end.
    pub fn push_vertex(&mut self, vertex: FeatureVertex) {
        self.vertices.push(vertex);
        self.modified = chrono::Utc::now();
    }

    /// Insert vertex at index.
    pub fn insert_vertex(&mut self, index: usize, vertex: FeatureVertex) {
        if index <= self.vertices.len() {
            self.vertices.insert(index, vertex);
            self.modified = chrono::Utc::now();
        }
    }

    /// Remove vertex at index.
    pub fn remove_vertex(&mut self, index: usize) -> Option<FeatureVertex> {
        if index < self.vertices.len() && self.vertices.len() > 2 {
            self.modified = chrono::Utc::now();
            Some(self.vertices.remove(index))
        } else {
            None
        }
    }

    /// Update vertex position.
    pub fn set_vertex(&mut self, index: usize, pt: Point3d) -> bool {
        if index < self.vertices.len() {
            self.vertices[index].pt = pt;
            self.modified = chrono::Utc::now();
            true
        } else {
            false
        }
    }

    /// Link to a surface.
    pub fn link_surface(
        &mut self,
        surface_name: String,
        surface_handle: u64,
        mode: LinkMode,
        surface: &crate::surface::Surface,
    ) {
        let mut link = SurfaceLink::new(surface_name, surface_handle, mode);

        // Create vertex links by finding containing triangles
        for (idx, vertex) in self.vertices.iter().enumerate() {
            if let Some((tri_idx, bary)) =
                surface.find_containing_triangle(vertex.pt.x, vertex.pt.y)
            {
                link.vertex_links.push(VertexLink {
                    vertex_index: idx,
                    triangle_idx: tri_idx,
                    barycentric: bary,
                    z_at_link: vertex.pt.z,
                    is_overridden: vertex.z_source == ZSource::UserEntered,
                });
            }
        }

        self.surface_links.push(link);
        self.modified = chrono::Utc::now();
    }

    /// Sync with linked surfaces.
    pub fn sync_surfaces(
        &mut self,
        surfaces: &std::collections::HashMap<String, crate::surface::Surface>,
    ) -> FeatureLineSyncResult {
        let mut report = FeatureLineSyncResult::default();

        // Collect indices of dynamic links first to avoid borrow issues
        let dynamic_indices: Vec<usize> = self
            .surface_links
            .iter()
            .enumerate()
            .filter(|(_, l)| l.link_mode == LinkMode::Dynamic)
            .map(|(i, _)| i)
            .collect();

        for idx in dynamic_indices {
            let link = &mut self.surface_links[idx];

            if let Some(surface) = surfaces.get(&link.surface_name) {
                let result = Self::sync_link_static(&mut self.vertices, link, surface);
                report.vertices_updated += result.vertices_updated;
                report.vertices_conflicted += result.vertices_conflicted;
                report.vertices_unchanged += result.vertices_unchanged;
                report.errors.extend(result.errors);
            } else {
                report
                    .errors
                    .push(format!("Surface '{}' not found", link.surface_name));
                link.sync_status = SyncStatus::Error("Surface not found".into());
            }
        }

        self.modified = chrono::Utc::now();
        report
    }

    fn sync_link_static(
        vertices: &mut [FeatureVertex],
        link: &mut SurfaceLink,
        surface: &crate::surface::Surface,
    ) -> FeatureLineSyncResult {
        let mut result = FeatureLineSyncResult::default();

        for vl in &link.vertex_links {
            if vl.is_overridden {
                result.vertices_conflicted += 1;
                continue;
            }

            // Get triangle
            if vl.triangle_idx >= surface.triangles.len() {
                result
                    .errors
                    .push(format!("Triangle index {} out of bounds", vl.triangle_idx));
                continue;
            }

            let tri = surface.triangles[vl.triangle_idx];
            let a = surface.nodes[tri[0]];
            let b = surface.nodes[tri[1]];
            let c = surface.nodes[tri[2]];

            // Barycentric interpolation
            let (u, v, w) = vl.barycentric;
            let new_z = u * a[2] + v * b[2] + w * c[2];

            if (new_z - vertices[vl.vertex_index].pt.z).abs() > 1e-9 {
                vertices[vl.vertex_index].pt.z = new_z;
                vertices[vl.vertex_index].z_source = ZSource::TINInterpolated;
                result.vertices_updated += 1;
            } else {
                result.vertices_unchanged += 1;
            }
        }

        link.last_sync = chrono::Utc::now();
        link.sync_status = if result.vertices_conflicted > 0 {
            SyncStatus::Conflict
        } else {
            SyncStatus::Synced
        };

        result
    }

    /// Serialize FeatureLine to JSON string for XDATA storage.
    /// The JSON can be stored as a string in XDATA record.
    pub fn to_xdata_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Deserialize FeatureLine from JSON string stored in XDATA.
    pub fn from_xdata_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Get the Z coordinates as a comma-separated string (for legacy XDATA compatibility).
    pub fn z_coords_csv(&self) -> String {
        self.vertices
            .iter()
            .map(|v| format!("{:.3}", v.pt.z))
            .collect::<Vec<_>>()
            .join(",")
    }

    /// Create a FeatureLine from vertices and Z coordinates CSV (for legacy compatibility).
    pub fn from_vertices_and_z_csv(
        name: String,
        vertices_xy: Vec<(f64, f64)>,
        z_csv: &str,
    ) -> Result<Self, String> {
        let z_values: Vec<f64> = z_csv
            .split(',')
            .map(|s| s.trim().parse::<f64>().map_err(|_| "Invalid Z value"))
            .collect::<Result<Vec<_>, _>>()?;

        if vertices_xy.len() != z_values.len() {
            return Err("Vertex count doesn't match Z count".into());
        }

        let mut fl = Self::new(name);
        for ((x, y), z) in vertices_xy.into_iter().zip(z_values) {
            fl.push_vertex(FeatureVertex::new_2d(x, y, z));
        }
        Ok(fl)
    }
}
