use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub type Color = [u8; 4];

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}
impl Vec3 {
    #[must_use]
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CameraSpec {
    pub position: Vec3,
    pub target: Vec3,
    pub fovy_deg: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeshStrip {
    pub left: Arc<[Vec3]>,
    pub right: Arc<[Vec3]>,
    pub color: Color,
    #[serde(default)]
    pub x_offset_m: f64,
}
impl MeshStrip {
    #[must_use]
    pub fn new(left: impl Into<Arc<[Vec3]>>, right: impl Into<Arc<[Vec3]>>, color: Color) -> Self {
        Self {
            left: left.into(),
            right: right.into(),
            color,
            x_offset_m: 0.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VehicleBox {
    pub center: Vec3,
    pub right_x: f64,
    pub right_y: f64,
    pub forward_x: f64,
    pub forward_y: f64,
    pub width_m: f64,
    pub length_m: f64,
    pub height_m: f64,
    pub body_color: Color,
    pub side_color: Color,
    pub rear_color: Color,
    pub top_highlight: Color,
    pub outline_color: Color,
    pub confidence: f64,
    pub label: String,
    pub source: String,
    pub longitudinal_m: Option<f64>,
    pub relative_speed_mps: Option<f64>,
    pub absolute_speed_kph: Option<f64>,
    pub acceleration_mps2: Option<f64>,
    pub ttc_s: Option<f64>,
    pub cut_in: bool,
    pub primary: bool,
    pub annotate: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RadarPointMarker {
    pub center: Vec3,
    pub radius_m: f64,
    pub color: Color,
    pub label: String,
    pub longitudinal_m: f64,
    pub lateral_m: f64,
    pub relative_speed_mps: Option<f64>,
    pub absolute_speed_kph: Option<f64>,
    pub lateral_speed_mps: Option<f64>,
    pub relative_accel_mps2: Option<f64>,
    pub probability: Option<f64>,
    pub valid: Option<i64>,
    pub in_my_lane: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PathBlocker {
    pub offset: f64,
    pub forward_m: f64,
    pub length_m: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClusterScene {
    pub camera: CameraSpec,
    pub scene_shift_x_m: f64,
    pub road_surface: MeshStrip,
    pub road_edges: Vec<MeshStrip>,
    pub highlight_lanes: Vec<MeshStrip>,
    pub lane_markings: Vec<MeshStrip>,
    pub planned_path: Vec<MeshStrip>,
    pub radar_points: Vec<RadarPointMarker>,
    pub vehicles: Vec<VehicleBox>,
}
