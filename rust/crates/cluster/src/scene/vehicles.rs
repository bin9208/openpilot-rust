use super::{
    geometry::{road_world_x, vehicle_heading},
    lines::{render_scene_forward_m, scene_data_relative_forward_m},
    math::{clamp, darken, lighten, rgba},
    radar_candidates::radar_point_display_lateral_m,
    radar_matching::radar_point_hidden_by_vehicle_box,
    radar_merge::radar_point_display_cmp,
    radar_motion::{
        radar_point_absolute_speed_kph, radar_point_display_lateral_speed_mps,
        radar_point_is_cross_traffic, radar_point_is_oncoming, radar_point_vehicle_heading,
        vehicle_heading_from_velocity,
    },
    vehicle_style::{
        radar_point_color, radar_point_radius, radar_vehicle_confidence, vehicle_color_for_source,
    },
    RadarPointMarker, Vec3, VehicleBox,
};
use crate::{
    config::{ClusterTheme, AMBER, RED, VEHICLE_HEIGHT_M, VEHICLE_LENGTH_M, VEHICLE_WIDTH_M},
    models::{ClusterUiState, RadarPoint},
};
use num_traits::ToPrimitive;

#[derive(Default)]
pub struct VehicleGeometry {
    pub offset: f64,
    pub forward: f64,
    pub steering: f64,
    pub lane_width: f64,
    pub target_offset: Option<f64>,
    pub x_offset: f64,
    pub center_x: Option<f64>,
    pub center_z_offset: f64,
}

pub struct VehicleMetadata {
    pub confidence: f64,
    pub label: String,
    pub source: String,
    pub longitudinal: Option<f64>,
    pub relative_speed: Option<f64>,
    pub absolute_speed: Option<f64>,
    pub lateral_speed: Option<f64>,
    pub acceleration: Option<f64>,
    pub ttc: Option<f64>,
    pub cut_in: bool,
    pub primary: bool,
    pub annotate: bool,
}

impl Default for VehicleMetadata {
    fn default() -> Self {
        Self {
            confidence: 1.0,
            label: String::new(),
            source: String::new(),
            longitudinal: None,
            relative_speed: None,
            absolute_speed: None,
            lateral_speed: None,
            acceleration: None,
            ttc: None,
            cut_in: false,
            primary: false,
            annotate: false,
        }
    }
}

#[must_use]
pub fn vehicle_box(
    geometry: &VehicleGeometry,
    color: [u8; 3],
    metadata: &VehicleMetadata,
) -> VehicleBox {
    let confidence = clamp(metadata.confidence, 0.0, 1.0);
    let alpha = (92.0 + 163.0 * confidence).to_u8().unwrap_or(255);
    let x = geometry.center_x.unwrap_or_else(|| {
        road_world_x(
            geometry.offset,
            geometry.forward,
            geometry.steering,
            geometry.lane_width,
        ) + geometry.x_offset
    });
    let heading = vehicle_heading(
        geometry.offset,
        geometry.forward,
        geometry.steering,
        geometry.lane_width,
        geometry.target_offset,
    );
    let (right_x, right_y, forward_x, forward_y) = vehicle_heading_from_velocity(
        metadata.absolute_speed,
        metadata.lateral_speed,
        heading,
        12.0,
        0.0,
    );
    VehicleBox {
        center: Vec3::new(
            x,
            geometry.forward,
            VEHICLE_HEIGHT_M * 0.5 + geometry.center_z_offset,
        ),
        right_x,
        right_y,
        forward_x,
        forward_y,
        width_m: VEHICLE_WIDTH_M,
        length_m: VEHICLE_LENGTH_M,
        height_m: VEHICLE_HEIGHT_M,
        body_color: rgba(color, alpha),
        side_color: rgba(darken(color, 0.20), alpha),
        rear_color: rgba(darken(color, 0.28), alpha),
        top_highlight: rgba(lighten(color, 0.16), alpha.min(235)),
        outline_color: rgba(darken(color, 0.42), alpha.min(235)),
        confidence,
        label: metadata.label.clone(),
        source: metadata.source.clone(),
        longitudinal_m: Some(
            metadata
                .longitudinal
                .unwrap_or_else(|| scene_data_relative_forward_m(geometry.forward)),
        ),
        relative_speed_mps: metadata.relative_speed,
        absolute_speed_kph: metadata.absolute_speed,
        acceleration_mps2: metadata.acceleration,
        ttc_s: metadata.ttc,
        cut_in: metadata.cut_in,
        primary: metadata.primary,
        annotate: metadata.annotate,
    }
}

#[must_use]
pub fn radar_vehicle_box(
    point: &RadarPoint,
    state: &ClusterUiState,
    width: f64,
    theme: &ClusterTheme,
    lateral_offset: f64,
) -> VehicleBox {
    let color = if radar_point_is_oncoming(point, state) {
        RED
    } else if radar_point_is_cross_traffic(point, lateral_offset) {
        AMBER
    } else {
        vehicle_color_for_source(&point.source, theme, state.radar_source_color_mode)
    };
    let mut forward = render_scene_forward_m(point.longitudinal_m, state);
    let mut x = radar_point_display_lateral_m(point, width);
    let (right_x, right_y, forward_x, forward_y) =
        radar_point_vehicle_heading(point, state, lateral_offset);
    if point.source == "cornerRadar" && state.surround_view_active {
        x += forward_x * VEHICLE_LENGTH_M * 0.5;
        forward += forward_y * VEHICLE_LENGTH_M * 0.5;
    }
    let geometry = VehicleGeometry {
        forward,
        lane_width: width,
        center_x: Some(x),
        ..VehicleGeometry::default()
    };
    let metadata = VehicleMetadata {
        confidence: radar_vehicle_confidence(point),
        label: point.label.clone(),
        source: point.source.clone(),
        longitudinal: Some(point.longitudinal_m),
        relative_speed: point.relative_speed_mps,
        absolute_speed: radar_point_absolute_speed_kph(point, state),
        acceleration: point.relative_accel_mps2,
        ..VehicleMetadata::default()
    };
    let mut vehicle = vehicle_box(&geometry, color, &metadata);
    vehicle.right_x = right_x;
    vehicle.right_y = right_y;
    vehicle.forward_x = forward_x;
    vehicle.forward_y = forward_y;
    vehicle
}

pub struct MarkerViewport {
    pub min_forward: f64,
    pub max_forward: f64,
    pub x_offset: f64,
    pub lateral_speed_offset: f64,
}

#[must_use]
pub fn radar_point_markers(
    state: &ClusterUiState,
    width: f64,
    vehicle_points: &[RadarPoint],
    viewport: &MarkerViewport,
) -> Vec<RadarPointMarker> {
    let mut points: Vec<_> = state.radar_points.iter().collect();
    points.sort_by(|left, right| radar_point_display_cmp(left, right, state));
    points
        .into_iter()
        .filter_map(|point| {
            if radar_point_hidden_by_vehicle_box(point, vehicle_points, state) {
                return None;
            }
            let forward = render_scene_forward_m(point.longitudinal_m, state);
            if forward < viewport.min_forward || forward > viewport.max_forward {
                return None;
            }
            Some(RadarPointMarker {
                center: Vec3::new(
                    radar_point_display_lateral_m(point, width) + viewport.x_offset,
                    forward,
                    0.20,
                ),
                radius_m: radar_point_radius(point),
                color: radar_point_color(point),
                label: point.label.clone(),
                longitudinal_m: point.longitudinal_m,
                lateral_m: point.lateral_m,
                relative_speed_mps: point.relative_speed_mps,
                absolute_speed_kph: radar_point_absolute_speed_kph(point, state),
                lateral_speed_mps: radar_point_display_lateral_speed_mps(
                    point,
                    viewport.lateral_speed_offset,
                ),
                relative_accel_mps2: point.relative_accel_mps2,
                probability: point.probability,
                valid: point.valid,
                in_my_lane: point.in_my_lane,
            })
        })
        .take(24)
        .collect()
}

#[must_use]
pub fn vehicle_box_with_x_offset(vehicle: &VehicleBox, offset: f64) -> VehicleBox {
    let mut shifted = vehicle.clone();
    shifted.center.x += offset;
    shifted
}

#[must_use]
pub fn translate_vehicle_box_x(vehicle: &VehicleBox, offset: f64) -> VehicleBox {
    if offset.abs() <= 0.0001 {
        vehicle.clone()
    } else {
        vehicle_box_with_x_offset(vehicle, offset)
    }
}

#[must_use]
pub fn translate_radar_marker_x(marker: &RadarPointMarker, offset: f64) -> RadarPointMarker {
    let mut shifted = marker.clone();
    if offset.abs() > 0.0001 {
        shifted.center.x += offset;
    }
    shifted
}
