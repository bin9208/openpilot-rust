use super::{
    CLUSTER_CAMERA_VIEW_MODE_AUTO_CAMERA, CLUSTER_CAMERA_VIEW_MODE_ROAD_CAMERA,
    CLUSTER_CAMERA_VIEW_MODE_WIDE_CAMERA,
};

#[must_use]
pub const fn cluster_camera_view_is_road_camera(mode: i64) -> bool {
    matches!(
        mode,
        CLUSTER_CAMERA_VIEW_MODE_ROAD_CAMERA
            | CLUSTER_CAMERA_VIEW_MODE_WIDE_CAMERA
            | CLUSTER_CAMERA_VIEW_MODE_AUTO_CAMERA
    )
}

pub use crate::rate::{
    prefers_wide as cluster_camera_view_prefers_wide, wide_zoom as cluster_wide_camera_zoom_factor,
};
