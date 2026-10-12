use super::{
    math::clamp,
    radar_motion::radar_point_absolute_speed_kph,
    vehicle_sources::{
        detected_vehicle_is_corner_summary, detected_vehicle_is_front_lead, CORNER_RADAR_LABELS,
    },
};
use crate::{
    config::CLUSTER_RADAR_DISPLAY_DETAIL,
    models::{ClusterUiState, DetectedVehicle, RadarPoint},
};

#[must_use]
pub fn radar_point_matches_corner_summary_distance(
    point: &RadarPoint,
    vehicle: &DetectedVehicle,
) -> bool {
    point.source == "cornerRadar"
        && detected_vehicle_is_corner_summary(vehicle)
        && (point.longitudinal_m - vehicle.longitudinal_m).abs()
            <= clamp(vehicle.longitudinal_m * 0.05, 2.0, 4.0)
}

#[must_use]
pub fn radar_point_close_to_detected_vehicle(
    point: &RadarPoint,
    vehicle: &DetectedVehicle,
) -> bool {
    if radar_point_matches_corner_summary_distance(point, vehicle) {
        return true;
    }
    let (longitudinal, lateral) = if detected_vehicle_is_front_lead(vehicle) {
        (
            clamp(
                point.longitudinal_m.max(vehicle.longitudinal_m) * 0.20,
                5.0,
                11.0,
            ),
            2.25,
        )
    } else {
        (clamp(point.longitudinal_m * 0.08, 4.0, 8.0), 1.35)
    };
    (point.longitudinal_m - vehicle.longitudinal_m).abs() <= longitudinal
        && (point.lateral_m - vehicle.lateral_m).abs() <= lateral
}

#[must_use]
pub fn radar_point_matches_detected_vehicle(point: &RadarPoint, state: &ClusterUiState) -> bool {
    state
        .detected_vehicles
        .iter()
        .any(|vehicle| radar_point_close_to_detected_vehicle(point, vehicle))
}

#[must_use]
pub fn radar_point_matches_front_lead(point: &RadarPoint, state: &ClusterUiState) -> bool {
    state.detected_vehicles.iter().any(|vehicle| {
        detected_vehicle_is_front_lead(vehicle)
            && radar_point_close_to_detected_vehicle(point, vehicle)
    })
}

#[must_use]
pub fn radar_point_hidden_by_detected_vehicle(
    point: &RadarPoint,
    vehicles: &[DetectedVehicle],
    state: &ClusterUiState,
) -> bool {
    state.radar_display_mode != CLUSTER_RADAR_DISPLAY_DETAIL
        && vehicles
            .iter()
            .any(|vehicle| radar_point_close_to_detected_vehicle(point, vehicle))
}

#[must_use]
pub fn radar_points_same_vehicle(left: &RadarPoint, right: &RadarPoint) -> bool {
    (left.longitudinal_m - right.longitudinal_m).abs() <= 7.0
        && (left.lateral_m - right.lateral_m).abs() <= 1.6
}

#[must_use]
pub fn radar_point_hidden_by_vehicle_box(
    point: &RadarPoint,
    vehicles: &[RadarPoint],
    state: &ClusterUiState,
) -> bool {
    vehicles.iter().any(|vehicle| {
        if state.radar_display_mode == CLUSTER_RADAR_DISPLAY_DETAIL {
            point.label == vehicle.label
        } else {
            radar_points_same_vehicle(point, vehicle)
        }
    })
}

#[must_use]
pub fn detected_vehicle_needs_radar_merge(vehicle: &DetectedVehicle) -> bool {
    (vehicle.source.starts_with("modelV2") && vehicle.longitudinal_m > 0.0)
        || (CORNER_RADAR_LABELS.contains(&vehicle.label.as_str())
            && vehicle.absolute_speed_kph.is_none()
            && (vehicle.source == "carState" || vehicle.source.starts_with("CAN 0x")))
}

#[must_use]
pub fn radar_point_close_to_vehicle(point: &RadarPoint, vehicle: &DetectedVehicle) -> bool {
    (point.longitudinal_m - vehicle.longitudinal_m).abs()
        <= clamp(vehicle.longitudinal_m * 0.08, 3.0, 7.0)
        && (point.lateral_m - vehicle.lateral_m).abs() <= 1.35
}

#[must_use]
pub fn radar_point_can_merge_with_vehicle(point: &RadarPoint, vehicle: &DetectedVehicle) -> bool {
    if vehicle.source.starts_with("modelV2") {
        radar_point_close_to_detected_vehicle(point, vehicle)
    } else {
        radar_point_close_to_vehicle(point, vehicle)
    }
}

#[must_use]
pub fn radar_merge_point_for_vehicle<'a>(
    vehicle: &DetectedVehicle,
    points: impl Iterator<Item = &'a RadarPoint>,
    state: &ClusterUiState,
) -> Option<&'a RadarPoint> {
    if !detected_vehicle_needs_radar_merge(vehicle) {
        return None;
    }
    points
        .filter(|point| {
            (radar_point_absolute_speed_kph(point, state).is_some()
                || point.relative_speed_mps.is_some())
                && radar_point_can_merge_with_vehicle(point, vehicle)
        })
        .min_by(|left, right| {
            (left.longitudinal_m - vehicle.longitudinal_m)
                .abs()
                .partial_cmp(&(right.longitudinal_m - vehicle.longitudinal_m).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    (left.lateral_m - vehicle.lateral_m)
                        .abs()
                        .partial_cmp(&(right.lateral_m - vehicle.lateral_m).abs())
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        })
}
