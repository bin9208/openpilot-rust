use super::{
    radar_candidates::corner_radar_point_is_in_ego_lane,
    radar_merge::{
        corner_radar_points_for_cluster_display, radar_points_for_display, raw_corner_radar_points,
    },
    vehicle_sources::{
        detected_vehicle_is_front_lead, detected_vehicle_is_lead_one_or_two,
        detected_vehicle_is_rear_car_state_summary, detected_vehicles_without_zero_radar_samples,
    },
};
use crate::{config::cluster_camera_view_is_road_camera, models::ClusterUiState};

#[must_use]
pub fn display_state(input: &ClusterUiState, width: f64) -> (ClusterUiState, bool) {
    let mut corner = raw_corner_radar_points(&input.radar_points);
    let raw_corner_active = !corner.is_empty();
    if raw_corner_active
        && cluster_camera_view_is_road_camera(input.camera_view_mode)
        && input
            .detected_vehicles
            .iter()
            .any(detected_vehicle_is_lead_one_or_two)
    {
        corner.retain(|point| !corner_radar_point_is_in_ego_lane(point, input, width));
    }
    let points = if raw_corner_active {
        corner_radar_points_for_cluster_display(&corner, input)
    } else {
        radar_points_for_display(input)
    };
    let mut vehicles = detected_vehicles_without_zero_radar_samples(&input.detected_vehicles);
    if raw_corner_active {
        vehicles.retain(|vehicle| {
            vehicle.cut_in
                || detected_vehicle_is_front_lead(vehicle)
                || detected_vehicle_is_rear_car_state_summary(vehicle)
        });
    }
    let cutin_tracks: Vec<_> = vehicles
        .iter()
        .filter(|vehicle| vehicle.cut_in)
        .filter_map(|vehicle| vehicle.radar_track_id)
        .collect();
    vehicles.retain(|vehicle| {
        vehicle.cut_in
            || !vehicle
                .radar_track_id
                .is_some_and(|track| cutin_tracks.contains(&track))
    });
    let lead_one_tracks: Vec<_> = vehicles
        .iter()
        .filter(|vehicle| vehicle.label.to_uppercase().starts_with("L1"))
        .filter_map(|vehicle| vehicle.radar_track_id)
        .collect();
    vehicles.retain(|vehicle| {
        !vehicle.label.to_uppercase().starts_with("L2")
            || !vehicle
                .radar_track_id
                .is_some_and(|track| lead_one_tracks.contains(&track))
    });
    let mut state = input.clone();
    state.radar_points = points;
    state.detected_vehicles = vehicles;
    (state, raw_corner_active)
}
