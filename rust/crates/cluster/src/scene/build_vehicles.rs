use super::{
    detected_merge::{detected_vehicles_for_display, detected_vehicles_with_merged_radar},
    lines::{detected_vehicle_scene_forward_m, render_scene_forward_m},
    math::clamp,
    radar_matching::radar_point_hidden_by_detected_vehicle,
    vehicle_sources::merged_radar_point_label,
    vehicle_style::{
        vehicle_badge_has_special_info, vehicle_blocks_path, vehicle_color_for_detection,
    },
    vehicles::{vehicle_box, vehicle_box_with_x_offset, VehicleGeometry, VehicleMetadata},
    PathBlocker, VehicleBox,
};
use crate::{
    config::{
        cluster_camera_view_is_road_camera, ClusterTheme, CLUSTER_RADAR_DISPLAY_DETAIL, EGO,
        EGO_FORWARD_M, VEHICLE_LENGTH_M,
    },
    models::{ClusterUiState, RadarPoint},
};
use std::collections::HashSet;

pub struct VehicleScene {
    pub vehicles: Vec<VehicleBox>,
    pub blockers: Vec<PathBlocker>,
    pub merged_radar_labels: HashSet<String>,
}

pub struct VehicleSceneInput<'a> {
    pub state: &'a ClusterUiState,
    pub width: f64,
    pub relative_x: f64,
    pub route_mode: bool,
    pub raw_corner_active: bool,
    pub radar_points: &'a [RadarPoint],
    pub radar_boxes: &'a [VehicleBox],
    pub theme: &'a ClusterTheme,
}

pub(super) fn marker_hidden_points(
    mut selected: Vec<RadarPoint>,
    state: &ClusterUiState,
    raw_corner: bool,
    merged: &HashSet<String>,
) -> Vec<RadarPoint> {
    if !raw_corner {
        selected.extend(
            state
                .radar_points
                .iter()
                .filter(|point| merged.contains(&point.label))
                .cloned(),
        );
    }
    selected
}

#[must_use]
fn ego_vehicle(state: &ClusterUiState, width: f64) -> VehicleBox {
    vehicle_box(
        &VehicleGeometry {
            offset: clamp(state.ego_lane_offset, -1.25, 1.25),
            forward: EGO_FORWARD_M - VEHICLE_LENGTH_M * 0.5,
            steering: state.steering,
            lane_width: width,
            target_offset: if state.lane_change_phase == "changing" {
                state.highlight_lane_offset
            } else {
                None
            },
            ..VehicleGeometry::default()
        },
        EGO,
        &VehicleMetadata::default(),
    )
}

#[must_use]
pub fn build_scene_vehicles(input: &VehicleSceneInput<'_>) -> VehicleScene {
    let state = input.state;
    let mut result = VehicleScene {
        vehicles: Vec::new(),
        blockers: Vec::new(),
        merged_radar_labels: HashSet::new(),
    };
    if !cluster_camera_view_is_road_camera(state.camera_view_mode) {
        result.vehicles.push(ego_vehicle(state, input.width));
    }
    if !input.route_mode {
        return result;
    }
    let detected =
        if input.raw_corner_active || state.radar_display_mode == CLUSTER_RADAR_DISPLAY_DETAIL {
            state.detected_vehicles.clone()
        } else {
            detected_vehicles_for_display(
                &detected_vehicles_with_merged_radar(
                    &state.detected_vehicles,
                    &state.radar_points,
                    state,
                    0.0,
                ),
                state,
            )
        };
    result.merged_radar_labels = detected
        .iter()
        .filter_map(merged_radar_point_label)
        .map(str::to_owned)
        .collect();
    for vehicle in &detected {
        let geometry = VehicleGeometry {
            offset: clamp(vehicle.lateral_m / input.width, -2.2, 2.2),
            forward: detected_vehicle_scene_forward_m(vehicle, state),
            steering: state.steering,
            lane_width: input.width,
            center_x: Some(vehicle.lateral_m + input.relative_x),
            center_z_offset: -0.3,
            ..VehicleGeometry::default()
        };
        let metadata = VehicleMetadata {
            confidence: vehicle.probability,
            label: vehicle.label.clone(),
            source: vehicle.source.clone(),
            longitudinal: Some(vehicle.longitudinal_m),
            relative_speed: vehicle.relative_speed_mps,
            absolute_speed: vehicle.absolute_speed_kph.or_else(|| {
                vehicle
                    .relative_speed_mps
                    .map(|relative| state.speed_kph + relative * 3.6)
            }),
            lateral_speed: vehicle.lateral_speed_mps,
            acceleration: vehicle.acceleration_mps2,
            ttc: vehicle.ttc_s,
            cut_in: vehicle.cut_in,
            primary: vehicle.primary,
            annotate: vehicle_badge_has_special_info(vehicle),
        };
        result.vehicles.push(vehicle_box(
            &geometry,
            vehicle_color_for_detection(vehicle, input.theme, state.radar_source_color_mode),
            &metadata,
        ));
    }
    result.blockers.extend(
        state
            .detected_vehicles
            .iter()
            .filter(|vehicle| vehicle_blocks_path(vehicle))
            .map(|vehicle| PathBlocker {
                offset: clamp(vehicle.lateral_m / input.width, -2.2, 2.2),
                forward_m: render_scene_forward_m(vehicle.longitudinal_m, state),
                length_m: VEHICLE_LENGTH_M,
            }),
    );
    for (point, vehicle) in input.radar_points.iter().zip(input.radar_boxes) {
        if result.merged_radar_labels.contains(&point.label)
            || radar_point_hidden_by_detected_vehicle(point, &detected, state)
        {
            continue;
        }
        result.blockers.push(PathBlocker {
            offset: clamp(vehicle.center.x / input.width, -2.2, 2.2),
            forward_m: vehicle.center.y,
            length_m: vehicle.length_m,
        });
        result
            .vehicles
            .push(vehicle_box_with_x_offset(vehicle, input.relative_x));
    }
    result
}
