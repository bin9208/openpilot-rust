use super::{
    math::clamp,
    path::model_path_lateral_at_forward,
    radar_edges::{
        radar_point_is_near_or_outside_road_edge, radar_point_is_outside_outer_lane,
        radar_point_matches_static_road_edge, radar_point_road_edge_outside_distance_m,
    },
    radar_matching::{
        radar_point_matches_detected_vehicle, radar_point_matches_front_lead,
        radar_points_same_vehicle,
    },
    radar_merge::radar_point_display_cmp,
    radar_motion::{
        radar_point_has_meaningful_motion, radar_point_is_stationary_object,
        radar_point_motion_speed_kph,
    },
};
use crate::models::{ClusterUiState, RadarPoint};

#[must_use]
pub fn radar_point_is_confirmed_vehicle_source(point: &RadarPoint) -> bool {
    let source = point.source.to_lowercase();
    source == "cornerradar" || source.contains("0x162") || source.contains("0x1ea")
}

#[must_use]
pub fn radar_point_is_center_raw_point(point: &RadarPoint, width: f64) -> bool {
    point.lateral_m.abs() <= width * 0.72
}

#[must_use]
pub fn radar_point_display_lateral_m(point: &RadarPoint, width: f64) -> f64 {
    let lateral = clamp(point.lateral_m, -width * 3.0, width * 3.0);
    if point.source != "liveTracks" {
        return lateral;
    }
    let magnitude = 0.0_f64.max(lateral.abs() - 0.35);
    if magnitude > 0.0 {
        magnitude.copysign(lateral)
    } else {
        0.0
    }
}

#[must_use]
pub fn corner_radar_point_is_in_ego_lane(
    point: &RadarPoint,
    state: &ClusterUiState,
    width: f64,
) -> bool {
    point.in_my_lane.map_or_else(
        || {
            (point.lateral_m
                - model_path_lateral_at_forward(state, point.longitudinal_m).unwrap_or(0.0))
            .abs()
                <= width * 0.5
        },
        |value| value > 0,
    )
}

#[must_use]
pub fn radar_point_has_vehicle_estimate(
    point: &RadarPoint,
    state: &ClusterUiState,
    width: f64,
) -> bool {
    radar_point_matches_detected_vehicle(point, state)
        || ((point.in_my_lane.is_some_and(|value| value > 0)
            || point.probability.is_some_and(|value| value >= 0.35))
            && point.lateral_m.abs() <= width * 2.75)
}

#[must_use]
pub fn radar_point_is_moving_raw_vehicle(
    point: &RadarPoint,
    state: &ClusterUiState,
    width: f64,
) -> bool {
    if radar_point_motion_speed_kph(point, state).is_none_or(|speed| speed < 8.0) {
        return false;
    }
    let count = point.valid_count.unwrap_or(20);
    let lanes = point.lateral_m.abs() / 0.1_f64.max(width);
    if lanes <= 0.72 {
        count >= 20
    } else if lanes <= 1.45 {
        count >= 24
    } else if lanes <= 2.65 {
        count >= 35
    } else {
        false
    }
}

#[must_use]
pub fn radar_point_has_stable_edge_vehicle_motion(
    point: &RadarPoint,
    state: &ClusterUiState,
    width: f64,
) -> bool {
    point.valid_count.unwrap_or(0) >= 20
        && radar_point_motion_speed_kph(point, state).is_some_and(|speed| speed >= 18.0)
        && !point
            .relative_accel_mps2
            .is_some_and(|accel| accel.abs() > 5.0)
        && point.lateral_m.abs() <= width * 2.75
}

#[must_use]
pub fn radar_point_should_keep_across_road_edge(
    point: &RadarPoint,
    state: &ClusterUiState,
    width: f64,
    outside: Option<f64>,
) -> bool {
    let stable = radar_point_has_stable_edge_vehicle_motion(point, state, width);
    if let Some(distance) = outside.filter(|distance| *distance > 0.85) {
        return distance <= 2.25 && stable;
    }
    point.valid_count.unwrap_or(0) >= 20
        && radar_point_motion_speed_kph(point, state).is_some_and(|speed| speed >= 18.0)
        && radar_point_is_near_or_outside_road_edge(point, state, width, outside)
        && (stable
            || radar_point_has_vehicle_estimate(point, state, width)
            || radar_point_is_moving_raw_vehicle(point, state, width))
}

#[must_use]
pub fn radar_point_is_side_static_reflection(
    point: &RadarPoint,
    state: &ClusterUiState,
    width: f64,
) -> bool {
    if state.speed_kph < 10.0
        || point.in_my_lane.is_some_and(|value| value > 0)
        || point.probability.is_some_and(|value| value >= 0.35)
        || point.lateral_m.abs() <= width * 0.58
        || point.relative_speed_mps.unwrap_or(0.0).abs() > 1.25
    {
        return false;
    }
    let lanes = point.lateral_m.abs() / 0.1_f64.max(width);
    let count = point.valid_count.unwrap_or(0);
    !((lanes <= 1.45 && count >= 24) || (lanes <= 2.65 && count >= 35))
}

#[must_use]
pub fn radar_point_is_vehicle_candidate(
    point: &RadarPoint,
    state: &ClusterUiState,
    width: f64,
) -> bool {
    if !(2.5..=150.0).contains(&point.longitudinal_m) || point.lateral_m.abs() > width * 2.75 {
        return false;
    }
    let outside = radar_point_road_edge_outside_distance_m(point, state, width);
    if point.longitudinal_m <= 10.0 && outside.is_some_and(|distance| distance > 0.0) {
        return false;
    }
    if radar_point_is_confirmed_vehicle_source(point) {
        return true;
    }
    let moving = radar_point_has_meaningful_motion(point, state);
    if radar_point_is_outside_outer_lane(point, state, width) && !moving {
        return false;
    }
    if radar_point_is_center_raw_point(point, width)
        && !radar_point_matches_front_lead(point, state)
        && !moving
    {
        return false;
    }
    if point
        .valid_count
        .is_some_and(|count| count < 20 || point.motion_consistent != Some(true))
    {
        return false;
    }
    if point.promotion_held {
        return true;
    }
    let stable_edge = radar_point_has_stable_edge_vehicle_motion(point, state, width)
        && radar_point_is_near_or_outside_road_edge(point, state, width, outside);
    if point
        .probability
        .is_some_and(|probability| probability < 0.20)
        && point.in_my_lane.unwrap_or(0) == 0
        && !stable_edge
        && !moving
    {
        return false;
    }
    let keep = radar_point_should_keep_across_road_edge(point, state, width, outside);
    if outside.is_some_and(|distance| distance > 0.0) && !keep && !moving {
        return false;
    }
    if radar_point_matches_detected_vehicle(point, state)
        || radar_point_has_vehicle_estimate(point, state, width)
        || moving
    {
        return true;
    }
    if radar_point_is_stationary_object(point, state)
        || radar_point_is_side_static_reflection(point, state, width)
        || (radar_point_matches_static_road_edge(point, state, width) && !keep)
    {
        return false;
    }
    radar_point_is_moving_raw_vehicle(point, state, width)
}

#[must_use]
pub fn radar_vehicle_points(state: &ClusterUiState, width: f64) -> Vec<RadarPoint> {
    let mut candidates: Vec<_> = state
        .radar_points
        .iter()
        .filter(|point| radar_point_is_vehicle_candidate(point, state, width))
        .collect();
    candidates.sort_by(|left, right| radar_point_display_cmp(left, right, state));
    let mut selected = Vec::new();
    for point in candidates {
        if selected
            .iter()
            .any(|existing| radar_points_same_vehicle(point, existing))
        {
            continue;
        }
        selected.push(point.clone());
        if selected.len() >= 10 {
            break;
        }
    }
    selected.sort_by(|left, right| {
        left.longitudinal_m
            .partial_cmp(&right.longitudinal_m)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    selected
}
