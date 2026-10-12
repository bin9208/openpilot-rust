use super::{
    math::clamp,
    radar_matching::radar_merge_point_for_vehicle,
    radar_motion::{radar_point_absolute_speed_kph, radar_point_display_lateral_speed_mps},
    vehicle_sources::{
        detected_vehicle_is_front_merge_candidate, detected_vehicles_without_zero_radar_samples,
        merged_detected_vehicle_source, ttc_from_relative_speed, vehicle_source_base,
        RADAR_MERGED_SOURCE_TAG,
    },
};
use crate::{
    config::CLUSTER_RADAR_DISPLAY_DETAIL,
    models::{ClusterUiState, DetectedVehicle, RadarPoint},
};
use std::collections::HashSet;

#[must_use]
pub fn detected_vehicles_same_front_vehicle(
    left: &DetectedVehicle,
    right: &DetectedVehicle,
) -> bool {
    detected_vehicle_is_front_merge_candidate(left)
        && detected_vehicle_is_front_merge_candidate(right)
        && vehicle_source_base(&left.source) != vehicle_source_base(&right.source)
        && (left.longitudinal_m - right.longitudinal_m).abs()
            <= clamp(
                left.longitudinal_m.max(right.longitudinal_m) * 0.20,
                5.0,
                11.0,
            )
        && (left.lateral_m - right.lateral_m).abs() <= 2.25
}

fn source_priority(vehicle: &DetectedVehicle) -> u8 {
    if vehicle.source == "radarState" {
        0
    } else if vehicle.primary {
        1
    } else if vehicle.source.starts_with("modelV2") {
        2
    } else if vehicle.source == "carState" || vehicle.source.starts_with("CAN 0x") {
        3
    } else {
        4
    }
}

#[must_use]
pub fn merge_detected_vehicle_for_display(
    base: &DetectedVehicle,
    other: &DetectedVehicle,
) -> DetectedVehicle {
    DetectedVehicle {
        source: merged_detected_vehicle_source(&base.source, &other.source).into(),
        probability: base.probability.max(other.probability),
        relative_speed_mps: base.relative_speed_mps.or(other.relative_speed_mps),
        absolute_speed_kph: base.absolute_speed_kph.or(other.absolute_speed_kph),
        lateral_speed_mps: base.lateral_speed_mps.or(other.lateral_speed_mps),
        acceleration_mps2: base.acceleration_mps2.or(other.acceleration_mps2),
        cut_in: base.cut_in || other.cut_in,
        primary: base.primary || other.primary,
        ttc_s: base.ttc_s.or(other.ttc_s),
        x_std_m: base.x_std_m.or(other.x_std_m),
        y_std_m: base.y_std_m.or(other.y_std_m),
        radar_track_id: base.radar_track_id.or(other.radar_track_id),
        ..base.clone()
    }
}

#[must_use]
pub fn detected_vehicles_for_display(
    vehicles: &[DetectedVehicle],
    state: &ClusterUiState,
) -> Vec<DetectedVehicle> {
    let mut vehicles = detected_vehicles_without_zero_radar_samples(vehicles);
    if state.radar_display_mode == CLUSTER_RADAR_DISPLAY_DETAIL || vehicles.len() < 2 {
        return vehicles;
    }
    vehicles.sort_by(|left, right| {
        source_priority(left)
            .cmp(&source_priority(right))
            .then_with(|| {
                left.longitudinal_m
                    .partial_cmp(&right.longitudinal_m)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                right
                    .probability
                    .partial_cmp(&left.probability)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    let mut selected = Vec::new();
    for vehicle in vehicles {
        if let Some(existing) = selected
            .iter_mut()
            .find(|existing| detected_vehicles_same_front_vehicle(&vehicle, existing))
        {
            *existing = merge_detected_vehicle_for_display(existing, &vehicle);
        } else {
            selected.push(vehicle);
        }
    }
    selected.sort_by(|left, right| {
        left.longitudinal_m
            .partial_cmp(&right.longitudinal_m)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    selected
}

#[must_use]
pub fn detected_vehicles_with_merged_radar(
    vehicles: &[DetectedVehicle],
    points: &[RadarPoint],
    state: &ClusterUiState,
    lateral_speed_offset: f64,
) -> Vec<DetectedVehicle> {
    let mut used = HashSet::new();
    vehicles
        .iter()
        .map(|vehicle| {
            let Some(point) = radar_merge_point_for_vehicle(
                vehicle,
                points
                    .iter()
                    .filter(|point| !used.contains(point.label.as_str())),
                state,
            ) else {
                return vehicle.clone();
            };
            used.insert(point.label.as_str());
            DetectedVehicle {
                source: format!("{}{RADAR_MERGED_SOURCE_TAG}{}", vehicle.source, point.label),
                relative_speed_mps: point.relative_speed_mps.or(vehicle.relative_speed_mps),
                absolute_speed_kph: radar_point_absolute_speed_kph(point, state)
                    .or(vehicle.absolute_speed_kph),
                lateral_speed_mps: radar_point_display_lateral_speed_mps(
                    point,
                    lateral_speed_offset,
                )
                .or(vehicle.lateral_speed_mps),
                acceleration_mps2: vehicle.acceleration_mps2.or(point.relative_accel_mps2),
                ttc_s: vehicle.ttc_s.or_else(|| {
                    ttc_from_relative_speed(vehicle.longitudinal_m, point.relative_speed_mps)
                }),
                ..vehicle.clone()
            }
        })
        .collect()
}
