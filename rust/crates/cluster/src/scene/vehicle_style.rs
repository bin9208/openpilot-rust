use super::{
    math::{clamp, rgba},
    vehicle_sources::{
        vehicle_source_is_adas, vehicle_source_is_camera, vehicle_source_is_front_radar,
        vehicle_source_is_radar_track,
    },
    Color,
};
use crate::{
    config::{
        ClusterTheme, AMBER, BLUE, BLUE_SOFT, CLUSTER_RADAR_SOURCE_COLOR_BY_SOURCE, GREEN, RED,
    },
    models::{DetectedVehicle, RadarPoint},
};
use num_traits::ToPrimitive;

#[must_use]
pub fn vehicle_color_for_source(source: &str, theme: &ClusterTheme, mode: i64) -> [u8; 3] {
    if mode != CLUSTER_RADAR_SOURCE_COLOR_BY_SOURCE {
        return theme.default_vehicle;
    }
    if vehicle_source_is_adas(source) {
        GREEN
    } else if vehicle_source_is_front_radar(source) {
        RED
    } else if vehicle_source_is_radar_track(source) {
        AMBER
    } else if vehicle_source_is_camera(source) {
        BLUE_SOFT
    } else if source.starts_with("modelV2") {
        BLUE
    } else {
        theme.default_vehicle
    }
}

#[must_use]
pub fn vehicle_color_for_detection(
    vehicle: &DetectedVehicle,
    theme: &ClusterTheme,
    mode: i64,
) -> [u8; 3] {
    if vehicle.cut_in {
        AMBER
    } else if vehicle.primary {
        theme.primary_vehicle
    } else if vehicle
        .absolute_speed_kph
        .is_some_and(|speed| speed <= -12.0)
    {
        RED
    } else if vehicle
        .lateral_speed_mps
        .is_some_and(|speed| speed.abs() >= 2.0)
    {
        AMBER
    } else {
        vehicle_color_for_source(&vehicle.source, theme, mode)
    }
}

#[must_use]
pub fn vehicle_blocks_path(vehicle: &DetectedVehicle) -> bool {
    vehicle.longitudinal_m > 0.0
        && !(vehicle.source.starts_with("modelV2") && vehicle.probability < 0.35)
}

#[must_use]
pub fn vehicle_badge_has_special_info(vehicle: &DetectedVehicle) -> bool {
    vehicle.cut_in
        || vehicle.ttc_s.is_some_and(|ttc| ttc < 9.9)
        || vehicle
            .acceleration_mps2
            .is_some_and(|accel| accel.abs() > 1.0)
}

#[must_use]
pub fn radar_vehicle_confidence(point: &RadarPoint) -> f64 {
    if let Some(probability) = point.probability {
        clamp(0.58 + probability * 0.38, 0.58, 0.96)
    } else if let Some(count) = point.valid_count {
        clamp(
            0.56 + count.min(120).to_f64().unwrap_or(0.0) / 120.0 * 0.36,
            0.56,
            0.92,
        )
    } else {
        0.72
    }
}

#[must_use]
pub fn radar_point_color(point: &RadarPoint) -> Color {
    if point.valid.is_some_and(|valid| valid <= 0) {
        return [116, 126, 136, 150];
    }
    if (point.longitudinal_m < 12.0 && point.lateral_m.abs() < 1.6)
        || point.absolute_speed_kph.is_some_and(|speed| speed <= -12.0)
    {
        return rgba(RED, 232);
    }
    if point
        .probability
        .is_some_and(|probability| probability < 0.25)
    {
        return [116, 126, 136, 150];
    }
    if matches!(point.source.as_str(), "cornerRadar" | "liveTracks") {
        return rgba(AMBER, 226);
    }
    if point.in_my_lane.is_some_and(|value| value > 0) {
        return rgba(BLUE, 226);
    }
    if point.relative_speed_mps.is_some_and(|speed| speed < -2.5) {
        return rgba(AMBER, 226);
    }
    [34, 150, 255, 208]
}

#[must_use]
pub fn radar_point_radius(point: &RadarPoint) -> f64 {
    clamp(
        0.105 + 0.07 * point.probability.unwrap_or(0.72),
        0.095,
        0.19,
    )
}
