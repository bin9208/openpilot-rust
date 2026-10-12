use super::camera_for_state;
use crate::{
    config::EGO_FORWARD_M,
    models::ClusterUiState,
    renderer::{
        tokens::{
            CAMERA_BACKGROUND_VERTICAL_BIAS, CAMERA_OVERLAY_DEFAULT_HEIGHT_M,
            CAMERA_OVERLAY_MIN_DEPTH_M,
        },
        Point, Rect,
    },
    scene::{math::clamp, Vec3},
};
use num_traits::AsPrimitive;
use openpilot_ui_application::onroad::{
    calibration::{multiply, rotation, Matrix},
    model_renderer::math::float,
};

#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct CameraOverlayProjection {
    pub dest: Rect,
    pub source: Rect,
    pub video_dest: Rect,
    pub camera_width: f64,
    pub camera_height: f64,
    pub focal_length: f64,
    pub zoom: f64,
    pub video_tx: f64,
    pub video_ty: f64,
    pub view_from_road: Matrix,
    pub camera_height_m: f64,
    pub wide_camera: bool,
}
pub(super) fn source_rect(value: Rect) -> Rect {
    Rect::new(
        f64::from(float(value.x)),
        f64::from(float(value.y)),
        f64::from(float(value.width)),
        f64::from(float(value.height)),
    )
}
#[must_use]
pub fn projection(
    state: &ClusterUiState,
    dest: Rect,
    wide: bool,
    pitch_offset_deg: f64,
) -> CameraOverlayProjection {
    let [roll, mut pitch, yaw] = state.camera_calibration_euler.unwrap_or([0.; 3]);
    pitch += pitch_offset_deg.to_radians();
    let device_from_road = multiply(
        rotation([roll, pitch, yaw]),
        [[1., 0., 0.], [0., -1., 0.], [0., 0., -1.]],
    );
    let view = [[0., 1., 0.], [0., 0., 1.], [1., 0., 0.]];
    let view_from_road = if wide {
        state.wide_camera_from_device_euler.map_or_else(
            || multiply(view, device_from_road),
            |euler| multiply(multiply(view, rotation(euler)), device_from_road),
        )
    } else {
        multiply(view, device_from_road)
    };
    let camera = camera_for_state(state, wide);
    let source_width = state
        .route_overlay
        .as_ref()
        .filter(|overlay| overlay.video_width > 0)
        .map_or(camera.width, |overlay| overlay.video_width.as_());
    let source_height = state
        .route_overlay
        .as_ref()
        .filter(|overlay| overlay.video_height > 0)
        .map_or(camera.height, |overlay| overlay.video_height.as_());
    let cx = camera.width * 0.5;
    let cy = camera.height * 0.5;
    let calib = multiply(
        [
            [camera.focal_length, 0., cx],
            [0., camera.focal_length, cy],
            [0., 0., 1.],
        ],
        view_from_road,
    );
    let kep = calib.map(|row| row[0] * 1000.);
    let mut zoom = (dest.width / camera.width).max(dest.height / camera.height);
    if wide {
        zoom *= crate::rate::wide_zoom(state.speed_kph);
    }
    let max_x_offset = (cx * zoom - dest.width * 0.5).max(0.);
    let max_y_offset = (cy * zoom - dest.height * 0.5).max(0.);
    let (x_offset, y_offset) = if kep[2].abs() > 1e-6 {
        (
            clamp((kep[0] / kep[2] - cx) * zoom, -max_x_offset, max_x_offset),
            clamp((kep[1] / kep[2] - cy) * zoom, -max_y_offset, max_y_offset),
        )
    } else {
        (0., 0.)
    };
    let video_tx = (dest.width * 0.5 + dest.x - x_offset) - cx * zoom;
    let rendered_height = camera.height * zoom;
    let vertical_translation = dest.y
        - y_offset
        - (rendered_height - dest.height).max(0.) * CAMERA_BACKGROUND_VERTICAL_BIAS;
    let camera_height_m = state
        .road_transform_trans
        .filter(|values| values[2].is_finite())
        .map_or(CAMERA_OVERLAY_DEFAULT_HEIGHT_M, |values| {
            clamp(values[2], 0.5, 3.)
        });
    CameraOverlayProjection {
        dest,
        source: source_rect(Rect::new(0., 0., source_width, source_height)),
        video_dest: source_rect(Rect::new(
            video_tx,
            vertical_translation,
            camera.width * zoom,
            rendered_height,
        )),
        camera_width: camera.width,
        camera_height: camera.height,
        focal_length: camera.focal_length,
        zoom,
        video_tx,
        video_ty: vertical_translation,
        view_from_road,
        camera_height_m,
        wide_camera: wide,
    }
}
#[must_use]
pub fn screen_xy(
    point: Vec3,
    projection: &CameraOverlayProjection,
    scene_shift: f64,
    z_offset: f64,
) -> Option<Point> {
    let forward = point.y - EGO_FORWARD_M;
    let left = -(point.x + scene_shift);
    let up = point.z + z_offset;
    if forward <= CAMERA_OVERLAY_MIN_DEPTH_M {
        return None;
    }
    let [row_x, row_y, row_z] = projection.view_from_road;
    let dot = |row: [f64; 3]| row[0] * forward + row[1] * left + row[2] * up;
    let (view_x, view_y, view_z) = (
        dot(row_x),
        dot(row_y) + projection.camera_height_m,
        dot(row_z),
    );
    if view_z <= CAMERA_OVERLAY_MIN_DEPTH_M
        || ![view_x, view_y, view_z]
            .iter()
            .all(|value| value.is_finite())
    {
        return None;
    }
    let x = projection.zoom
        * (projection.focal_length * view_x / view_z + projection.camera_width * 0.5)
        + projection.video_tx;
    let y = projection.zoom
        * (projection.focal_length * view_y / view_z + projection.camera_height * 0.5)
        + projection.video_ty;
    let dest = projection.dest;
    if x < dest.x - 60.
        || x > dest.x + dest.width + 60.
        || y < dest.y - 60.
        || y > dest.y + dest.height + 60.
    {
        return None;
    }
    Some(Point::new(x, y))
}
