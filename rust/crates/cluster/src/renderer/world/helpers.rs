use super::super::{
    tokens::{WORLD_LABEL_FAR_M, WORLD_LABEL_MIN_SCALE, WORLD_LABEL_NEAR_M},
    Rect,
};
use crate::{
    config::ClusterTheme,
    scene::{CameraSpec, Vec3},
};

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct Bounds {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}
impl Bounds {
    #[must_use]
    pub const fn new(left: f64, top: f64, right: f64, bottom: f64) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }
}

#[must_use]
pub fn camera_forward(camera: &CameraSpec) -> Option<Vec3> {
    let dx = camera.target.x - camera.position.x;
    let dy = camera.target.y - camera.position.y;
    let dz = camera.target.z - camera.position.z;
    let length = (dx * dx + dy * dy + dz * dz).sqrt();
    if length <= 0.0001 || ![dx, dy, dz, length].into_iter().all(f64::is_finite) {
        return None;
    }
    Some(Vec3::new(dx / length, dy / length, dz / length))
}
#[must_use]
pub fn camera_depth_m(point: Vec3, camera: &CameraSpec) -> Option<f64> {
    let forward = camera_forward(camera)?;
    let dx = point.x - camera.position.x;
    let dy = point.y - camera.position.y;
    let dz = point.z - camera.position.z;
    if ![dx, dy, dz].into_iter().all(f64::is_finite) {
        return None;
    }
    Some(dx * forward.x + dy * forward.y + dz * forward.z)
}
#[must_use]
pub fn world_label_scale(distance: f64) -> f64 {
    let far = crate::scene::math::clamp(
        (distance.abs() - WORLD_LABEL_NEAR_M) / (WORLD_LABEL_FAR_M - WORLD_LABEL_NEAR_M),
        0.,
        1.,
    );
    1. - far * (1. - WORLD_LABEL_MIN_SCALE)
}
#[must_use]
pub fn rectangles_overlap(left: Rect, right: Rect) -> bool {
    left.x < right.x + right.width
        && left.x + left.width > right.x
        && left.y < right.y + right.height
        && left.y + left.height > right.y
}
#[must_use]
pub fn label_rect_inside_bounds(rect: Rect, bounds: Bounds) -> bool {
    [
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        bounds.left,
        bounds.top,
        bounds.right,
        bounds.bottom,
    ]
    .into_iter()
    .all(f64::is_finite)
        && rect.x >= bounds.left
        && rect.y >= bounds.top
        && rect.x + rect.width <= bounds.right
        && rect.y + rect.height <= bounds.bottom
}
#[must_use]
pub fn world_label_bounds(dimensions: (f64, f64), margins: Bounds) -> Bounds {
    let (width, height) = dimensions;
    Bounds::new(
        margins.left * width / super::DESIGN_WIDTH,
        margins.top * height / super::DESIGN_HEIGHT,
        width - margins.right * width / super::DESIGN_WIDTH,
        height - margins.bottom * height / super::DESIGN_HEIGHT,
    )
}
#[must_use]
pub fn vehicle_metric_color(
    vehicle: &crate::scene::VehicleBox,
    theme: &ClusterTheme,
    mode: i64,
) -> [u8; 3] {
    use crate::{
        config::{AMBER, BLUE, BLUE_SOFT, CLUSTER_RADAR_SOURCE_COLOR_BY_SOURCE, GREEN, RED},
        scene::{
            vehicle_source_is_adas, vehicle_source_is_camera, vehicle_source_is_front_radar,
            vehicle_source_is_radar_track,
        },
    };
    if vehicle.cut_in {
        return AMBER;
    }
    if vehicle.primary {
        return theme.primary_vehicle;
    }
    if mode != CLUSTER_RADAR_SOURCE_COLOR_BY_SOURCE {
        return theme.world_label_text;
    }
    if vehicle_source_is_adas(&vehicle.source) {
        GREEN
    } else if vehicle_source_is_front_radar(&vehicle.source) {
        RED
    } else if vehicle_source_is_radar_track(&vehicle.source) {
        AMBER
    } else if vehicle_source_is_camera(&vehicle.source) {
        BLUE_SOFT
    } else if vehicle.source.starts_with("modelV2") {
        BLUE
    } else {
        theme.world_label_text
    }
}
