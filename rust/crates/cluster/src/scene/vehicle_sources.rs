use crate::models::{radar_position_is_zero, DetectedVehicle};

pub const RADAR_MERGED_SOURCE_TAG: &str = "+radar:";
pub const CORNER_RADAR_LABELS: [&str; 4] = ["LF", "RF", "LR", "RR"];

#[must_use]
pub fn vehicle_source_base(source: &str) -> &str {
    source
        .split_once(RADAR_MERGED_SOURCE_TAG)
        .map_or(source, |(base, _)| base)
}

#[must_use]
pub fn vehicle_source_is_adas(source: &str) -> bool {
    matches!(
        vehicle_source_base(source),
        "carState" | "CAN 0x162" | "CAN 0x1ea"
    )
}

#[must_use]
pub fn vehicle_source_is_camera(source: &str) -> bool {
    vehicle_source_base(source).starts_with("camera")
}

#[must_use]
pub fn vehicle_source_is_front_radar(source: &str) -> bool {
    vehicle_source_base(source) == "radarState"
}

#[must_use]
pub fn vehicle_source_is_radar_track(source: &str) -> bool {
    matches!(source, "radarPoint" | "liveTracks" | "cornerRadar")
        || source.contains(RADAR_MERGED_SOURCE_TAG)
}

#[must_use]
pub fn detected_vehicle_is_front_lead(vehicle: &DetectedVehicle) -> bool {
    vehicle.source == "radarState" || matches!(vehicle.label.as_str(), "TARGET" | "TARGET2")
}

#[must_use]
pub fn detected_vehicle_is_lead_one_or_two(vehicle: &DetectedVehicle) -> bool {
    let label = vehicle.label.to_uppercase();
    label.starts_with("L1") || label.starts_with("L2")
}

#[must_use]
pub fn detected_vehicle_is_corner_summary(vehicle: &DetectedVehicle) -> bool {
    CORNER_RADAR_LABELS.contains(&vehicle.label.as_str()) && vehicle_source_is_adas(&vehicle.source)
}

#[must_use]
pub fn detected_vehicle_is_rear_corner_summary(vehicle: &DetectedVehicle) -> bool {
    matches!(vehicle.label.as_str(), "LR" | "RR") && vehicle_source_is_adas(&vehicle.source)
}

#[must_use]
pub fn detected_vehicle_is_rear_car_state_summary(vehicle: &DetectedVehicle) -> bool {
    matches!(vehicle.label.as_str(), "LR" | "RR")
        && vehicle_source_base(&vehicle.source) == "carState"
}

#[must_use]
pub fn detected_vehicle_is_zero_radar_sample(vehicle: &DetectedVehicle) -> bool {
    radar_position_is_zero(vehicle.longitudinal_m, vehicle.lateral_m)
        && (matches!(
            vehicle.source.as_str(),
            "radarState" | "carState" | "radarPoint" | "liveTracks" | "cornerRadar"
        ) || vehicle.source.starts_with("CAN 0x"))
}

#[must_use]
pub fn detected_vehicles_without_zero_radar_samples(
    vehicles: &[DetectedVehicle],
) -> Vec<DetectedVehicle> {
    vehicles
        .iter()
        .filter(|vehicle| !detected_vehicle_is_zero_radar_sample(vehicle))
        .cloned()
        .collect()
}

#[must_use]
pub fn detected_vehicle_is_front_merge_candidate(vehicle: &DetectedVehicle) -> bool {
    vehicle.longitudinal_m > 0.0
        && !CORNER_RADAR_LABELS.contains(&vehicle.label.as_str())
        && (detected_vehicle_is_front_lead(vehicle)
            || vehicle.source.starts_with("modelV2")
            || vehicle.primary)
}

#[must_use]
pub fn detected_vehicles_have_same_radar_track(
    left: &DetectedVehicle,
    right: &DetectedVehicle,
) -> bool {
    left.radar_track_id.is_some() && left.radar_track_id == right.radar_track_id
}

#[must_use]
pub fn merged_detected_vehicle_source<'a>(base: &'a str, other: &'a str) -> &'a str {
    for predicate in [
        vehicle_source_is_adas,
        vehicle_source_is_front_radar,
        vehicle_source_is_radar_track,
    ] {
        if predicate(base) {
            return base;
        }
        if predicate(other) {
            return other;
        }
    }
    base
}

#[must_use]
pub fn merged_radar_point_label(vehicle: &DetectedVehicle) -> Option<&str> {
    vehicle
        .source
        .rsplit_once(RADAR_MERGED_SOURCE_TAG)
        .map(|(_, label)| label)
        .filter(|label| !label.is_empty())
}

#[must_use]
pub fn ttc_from_relative_speed(longitudinal: f64, relative_speed: Option<f64>) -> Option<f64> {
    let speed = relative_speed?;
    if speed >= -0.15 || longitudinal <= 0.0 {
        return None;
    }
    Some(99.9_f64.min(longitudinal / 0.15_f64.max(-speed)))
}
