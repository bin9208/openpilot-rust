use super::math::{clamp, normalize2};
use crate::models::{ClusterUiState, RadarPoint};

pub const RADAR_STATIC_OBJECT_SPEED_KPH: f64 = 8.0;

#[must_use]
pub fn radar_point_absolute_speed_kph(point: &RadarPoint, state: &ClusterUiState) -> Option<f64> {
    point.absolute_speed_kph.or_else(|| {
        point
            .relative_speed_mps
            .map(|relative| state.speed_kph + relative * 3.6)
    })
}

#[must_use]
pub fn radar_point_motion_speed_kph(point: &RadarPoint, state: &ClusterUiState) -> Option<f64> {
    let lateral = point.lateral_speed_mps.map(|value| value * 3.6);
    radar_point_absolute_speed_kph(point, state).map_or_else(
        || lateral.map(f64::abs),
        |speed| Some(speed.hypot(lateral.unwrap_or(0.0))),
    )
}

#[must_use]
pub fn radar_point_has_meaningful_motion(point: &RadarPoint, state: &ClusterUiState) -> bool {
    point.valid.is_none_or(|value| value > 0)
        && radar_point_motion_speed_kph(point, state).is_some_and(|speed| speed >= 12.0)
}

#[must_use]
pub fn radar_point_is_oncoming(point: &RadarPoint, state: &ClusterUiState) -> bool {
    radar_point_absolute_speed_kph(point, state).is_some_and(|speed| speed <= -12.0)
}

#[must_use]
pub fn radar_point_display_lateral_speed_mps(point: &RadarPoint, offset: f64) -> Option<f64> {
    point.lateral_speed_mps.map(|speed| {
        if point.source == "cornerRadar" {
            speed - offset
        } else {
            speed
        }
    })
}

#[must_use]
pub fn radar_point_is_cross_traffic(point: &RadarPoint, offset: f64) -> bool {
    radar_point_display_lateral_speed_mps(point, offset).is_some_and(|speed| speed.abs() >= 2.0)
}

#[must_use]
pub fn radar_point_heading_lateral_speed_mps(
    point: &RadarPoint,
    state: &ClusterUiState,
    offset: f64,
) -> Option<f64> {
    let speed = radar_point_display_lateral_speed_mps(point, offset)?;
    if point.source != "cornerRadar" {
        return Some(speed);
    }
    let Some(yaw) = state
        .vision_yaw_rate_rps
        .filter(|yaw| yaw.is_finite() && yaw.abs() < 0.35)
    else {
        return Some(speed);
    };
    Some(speed + clamp(yaw * clamp(point.longitudinal_m, 0.0, 120.0), -3.0, 3.0))
}

#[must_use]
pub fn corner_radar_common_lateral_speed_mps(points: &[RadarPoint], state: &ClusterUiState) -> f64 {
    let mut candidates: Vec<_> = points
        .iter()
        .filter(|point| {
            point.source == "cornerRadar"
                && point.valid.is_none_or(|value| value > 0)
                && radar_point_absolute_speed_kph(point, state)
                    .is_some_and(|speed| speed.abs() <= 8.0)
        })
        .filter_map(|point| point.lateral_speed_mps)
        .collect();
    if candidates.len() < 2 {
        return 0.0;
    }
    candidates.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let middle = candidates.len() / 2;
    let median = if candidates.len() % 2 == 0 {
        candidates[middle - 1].midpoint(candidates[middle])
    } else {
        candidates[middle]
    };
    clamp(median * 0.25, -3.5, 3.5)
}

#[must_use]
pub fn vehicle_heading_from_velocity(
    longitudinal: Option<f64>,
    lateral: Option<f64>,
    fallback: (f64, f64, f64, f64),
    min_speed: f64,
    min_component: f64,
) -> (f64, f64, f64, f64) {
    if longitudinal.is_none() && lateral.is_none() {
        return fallback;
    }
    let forward = longitudinal.unwrap_or(0.0) / 3.6;
    let lateral = lateral.unwrap_or(0.0);
    if min_component > 0.0 && (forward.abs() < min_component || lateral.abs() < min_component) {
        return fallback;
    }
    if forward.hypot(lateral) * 3.6 < min_speed {
        return fallback;
    }
    let (x, y) = normalize2(lateral, forward);
    (y, -x, x, y)
}

#[must_use]
pub fn radar_point_vehicle_heading(
    point: &RadarPoint,
    state: &ClusterUiState,
    offset: f64,
) -> (f64, f64, f64, f64) {
    let corner = point.source == "cornerRadar";
    vehicle_heading_from_velocity(
        radar_point_absolute_speed_kph(point, state),
        radar_point_heading_lateral_speed_mps(point, state, offset),
        (1.0, 0.0, 0.0, 1.0),
        if corner { 1.0 } else { 12.0 },
        if corner { 0.5 } else { 0.0 },
    )
}

#[must_use]
pub fn radar_point_is_stationary_object(point: &RadarPoint, state: &ClusterUiState) -> bool {
    radar_point_absolute_speed_kph(point, state).is_some_and(|speed| speed.abs() <= 8.0)
}
