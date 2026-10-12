use super::{projection::source_rect, screen_xy, CameraOverlayProjection};
use crate::{
    config::{RADAR_TO_CAMERA_M, VEHICLE_LENGTH_M},
    renderer::{
        display::{display_speed, format_radar_distance, speed_unit},
        tokens::{
            CAMERA_OVERLAY_FRAME_EDGE_PAD_PX, CAMERA_OVERLAY_FRAME_MAX_ASPECT,
            CAMERA_OVERLAY_FRAME_MAX_HEIGHT_RATIO, CAMERA_OVERLAY_FRAME_MAX_WIDTH_RATIO,
            CAMERA_OVERLAY_FRAME_MIN_SIZE_PX, CAMERA_OVERLAY_VEHICLE_ROAD_HEIGHT_M,
        },
        world::{
            camera_overlay_vehicle_is_stopped, radar_info_shows_distance, radar_info_shows_speed,
            vehicle_distance_m,
        },
        Rect,
    },
    scene::{math::clamp, Vec3, VehicleBox},
};

#[must_use]
pub fn vehicle_draw_key(vehicle: &VehicleBox) -> (i32, f64) {
    let label = vehicle.label.to_uppercase();
    let priority = if vehicle.primary || label.starts_with("L1") {
        5
    } else if vehicle.cut_in || label.starts_with("L2") || label.contains("CUT-IN") {
        4
    } else if vehicle.source == "cornerRadar" {
        3
    } else {
        2
    };
    (priority, -vehicle_distance_m(vehicle))
}
#[must_use]
pub fn vehicle_overlay_label(vehicle: &VehicleBox, info_mode: i64, metric: bool) -> String {
    let stopped = camera_overlay_vehicle_is_stopped(vehicle);
    let mut parts = Vec::new();
    if !vehicle.label.is_empty() && (vehicle.primary || vehicle.cut_in) {
        let label = vehicle.label.to_uppercase();
        if !label.starts_with("L1") && !label.starts_with("L2") && !label.contains("CUT-IN") {
            parts.push(vehicle.label.clone());
        }
    }
    if stopped || radar_info_shows_distance(info_mode) {
        if let Some(distance) = vehicle.longitudinal_m {
            parts.push(format_radar_distance(distance, metric));
        }
    }
    if !stopped && radar_info_shows_speed(info_mode) {
        if let Some(speed) = vehicle.absolute_speed_kph {
            parts.push(format!(
                "{:.0} {}",
                display_speed(speed, metric),
                speed_unit(metric)
            ));
        }
    }
    parts.join(" ")
}
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct VehicleFrame {
    pub marker: Rect,
    pub lead_one: bool,
    pub lead_two: bool,
}
#[must_use]
pub fn vehicle_frame(
    vehicle: &VehicleBox,
    view: &CameraOverlayProjection,
    shift: f64,
    z_offset: f64,
) -> Option<VehicleFrame> {
    let center_y = vehicle.center.y + RADAR_TO_CAMERA_M + VEHICLE_LENGTH_M;
    let base_z = CAMERA_OVERLAY_VEHICLE_ROAD_HEIGHT_M;
    screen_xy(
        Vec3::new(vehicle.center.x, center_y, base_z),
        view,
        shift,
        z_offset,
    )?;
    let label = vehicle.label.to_uppercase();
    let lead_two = label.starts_with("L2") || label.contains("CUT-IN");
    let lead_one = label.starts_with("L1") || vehicle.primary && !lead_two;
    let half_width = (vehicle.width_m * 0.58).max(0.6);
    let height_m = (vehicle.height_m * 1.12).max(0.8);
    let corners = [
        Vec3::new(vehicle.center.x - half_width, center_y, base_z),
        Vec3::new(vehicle.center.x + half_width, center_y, base_z),
        Vec3::new(vehicle.center.x - half_width, center_y, base_z + height_m),
        Vec3::new(vehicle.center.x + half_width, center_y, base_z + height_m),
    ];
    let [a, b, c, d] = corners.map(|point| screen_xy(point, view, shift, z_offset));
    let [a, b, c, d] = [a?, b?, c?, d?];
    let min_x = a.x.min(b.x).min(c.x).min(d.x);
    let max_x = a.x.max(b.x).max(c.x).max(d.x);
    let min_y = a.y.min(b.y).min(c.y).min(d.y);
    let max_y = a.y.max(b.y).max(c.y).max(d.y);
    let width = max_x - min_x;
    let height = max_y - min_y;
    if !width.is_finite() || !height.is_finite() || width <= 0. || height <= 0. {
        return None;
    }
    let pad_x = (width * 0.08).max(4.);
    let pad_y = (height * 0.08).max(4.);
    let mut frame_width = clamp(
        width + pad_x * 2.,
        CAMERA_OVERLAY_FRAME_MIN_SIZE_PX,
        view.dest.width * CAMERA_OVERLAY_FRAME_MAX_WIDTH_RATIO,
    );
    let mut frame_height = clamp(
        height + pad_y * 2.,
        CAMERA_OVERLAY_FRAME_MIN_SIZE_PX,
        view.dest.height * CAMERA_OVERLAY_FRAME_MAX_HEIGHT_RATIO,
    );
    if frame_width > frame_height * CAMERA_OVERLAY_FRAME_MAX_ASPECT {
        frame_width = frame_height * CAMERA_OVERLAY_FRAME_MAX_ASPECT;
    } else if frame_height > frame_width * CAMERA_OVERLAY_FRAME_MAX_ASPECT {
        frame_height = frame_width * CAMERA_OVERLAY_FRAME_MAX_ASPECT;
    }
    let center_x = (min_x + max_x) * 0.5;
    let center_y = (min_y + max_y) * 0.5;
    let available_width = (center_x - view.dest.x - CAMERA_OVERLAY_FRAME_EDGE_PAD_PX)
        .min(view.dest.x + view.dest.width - center_x - CAMERA_OVERLAY_FRAME_EDGE_PAD_PX);
    let available_height = (center_y - view.dest.y - CAMERA_OVERLAY_FRAME_EDGE_PAD_PX)
        .min(view.dest.y + view.dest.height - center_y - CAMERA_OVERLAY_FRAME_EDGE_PAD_PX);
    if available_width < CAMERA_OVERLAY_FRAME_MIN_SIZE_PX * 0.5
        || available_height < CAMERA_OVERLAY_FRAME_MIN_SIZE_PX * 0.5
    {
        return None;
    }
    frame_width = frame_width.min(available_width * 2.);
    frame_height = frame_height.min(available_height * 2.);
    Some(VehicleFrame {
        marker: source_rect(Rect::new(
            center_x - frame_width * 0.5,
            center_y - frame_height * 0.5,
            frame_width,
            frame_height,
        )),
        lead_one,
        lead_two,
    })
}
