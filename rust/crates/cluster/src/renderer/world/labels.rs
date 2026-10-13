use super::super::display::{display_speed, format_radar_distance, speed_unit};
use crate::{
    config::{
        CLUSTER_RADAR_INFO_ALL_SPEED, CLUSTER_RADAR_INFO_ALL_SPEED_DISTANCE,
        CLUSTER_RADAR_INFO_NONE, CLUSTER_RADAR_INFO_VEHICLE_SPEED,
        CLUSTER_RADAR_INFO_VEHICLE_SPEED_DISTANCE, EGO_FORWARD_M,
    },
    scene::{RadarPointMarker, VehicleBox, RADAR_STATIC_OBJECT_SPEED_KPH},
};
#[must_use]
pub fn vehicle_distance_m(vehicle: &VehicleBox) -> f64 {
    vehicle
        .longitudinal_m
        .unwrap_or(vehicle.center.y - EGO_FORWARD_M)
}
#[must_use]
pub fn vehicle_has_lead_one_metrics(vehicle: &VehicleBox) -> bool {
    vehicle.primary && vehicle.label.to_uppercase() == "L1" && vehicle_distance_m(vehicle) > 0.
}
#[must_use]
pub fn vehicle_distance_label(vehicle: &VehicleBox, metric: bool) -> String {
    if vehicle
        .absolute_speed_kph
        .is_some_and(|speed| speed.abs() <= RADAR_STATIC_OBJECT_SPEED_KPH)
        && !vehicle.primary
        && !vehicle.cut_in
    {
        return String::new();
    }
    let distance = format_radar_distance(vehicle_distance_m(vehicle), metric);
    if vehicle.cut_in {
        return format!("CUT-IN {distance}");
    }
    if (vehicle.primary || vehicle.cut_in) && !vehicle.label.is_empty() {
        let label = vehicle.label.to_uppercase();
        if label.starts_with("L1") || label.starts_with("L2") {
            return distance;
        }
        return format!("{} {distance}", vehicle.label);
    }
    distance
}
fn speed_label(speed: Option<f64>, metric: bool) -> String {
    match speed {
        Some(speed) if speed.abs() <= RADAR_STATIC_OBJECT_SPEED_KPH => String::new(),
        Some(speed) => {
            format!("{:.0} {}", display_speed(speed, metric), speed_unit(metric))
        }
        None => String::new(),
    }
}
#[must_use]
pub fn vehicle_speed_label(vehicle: &VehicleBox, metric: bool) -> String {
    speed_label(vehicle.absolute_speed_kph, metric)
}
#[must_use]
pub fn radar_point_distance_label(point: &RadarPointMarker, metric: bool) -> String {
    if point
        .absolute_speed_kph
        .is_some_and(|speed| speed.abs() <= RADAR_STATIC_OBJECT_SPEED_KPH)
    {
        String::new()
    } else {
        format_radar_distance(point.longitudinal_m, metric)
    }
}
#[must_use]
pub fn radar_point_speed_label(point: &RadarPointMarker, metric: bool) -> String {
    speed_label(point.absolute_speed_kph, metric)
}
#[must_use]
pub fn radar_info_shows_vehicle(mode: i64) -> bool {
    matches!(
        mode,
        CLUSTER_RADAR_INFO_VEHICLE_SPEED
            | CLUSTER_RADAR_INFO_VEHICLE_SPEED_DISTANCE
            | CLUSTER_RADAR_INFO_ALL_SPEED
            | CLUSTER_RADAR_INFO_ALL_SPEED_DISTANCE
    )
}
#[must_use]
pub fn radar_info_shows_radar_points(mode: i64) -> bool {
    matches!(
        mode,
        CLUSTER_RADAR_INFO_ALL_SPEED | CLUSTER_RADAR_INFO_ALL_SPEED_DISTANCE
    )
}
#[must_use]
pub fn radar_info_shows_speed(mode: i64) -> bool {
    mode != CLUSTER_RADAR_INFO_NONE
}
#[must_use]
pub fn radar_info_shows_distance(mode: i64) -> bool {
    matches!(
        mode,
        CLUSTER_RADAR_INFO_VEHICLE_SPEED_DISTANCE | CLUSTER_RADAR_INFO_ALL_SPEED_DISTANCE
    )
}
