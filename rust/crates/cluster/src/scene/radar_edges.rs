use super::{math::clamp, radar_motion::radar_point_absolute_speed_kph};
use crate::models::{ClusterUiState, ModelPathPoint, RadarPoint};

#[must_use]
pub fn model_line_lateral_at(points: &[ModelPathPoint], forward: f64, shift: f64) -> Option<f64> {
    let first = points.first()?;
    if forward <= first.forward_m {
        return Some(first.lateral_m + shift);
    }
    for pair in points.windows(2) {
        let (left, right) = (&pair[0], &pair[1]);
        if left.forward_m <= forward && forward <= right.forward_m {
            let amount = clamp(
                (forward - left.forward_m) / 0.001_f64.max(right.forward_m - left.forward_m),
                0.0,
                1.0,
            );
            return Some(left.lateral_m + (right.lateral_m - left.lateral_m) * amount + shift);
        }
    }
    points.last().map(|point| point.lateral_m + shift)
}

#[must_use]
pub fn road_edge_lateral_at(
    points: &[ModelPathPoint],
    shift: f64,
    offset: Option<f64>,
    forward: f64,
    width: f64,
) -> Option<f64> {
    model_line_lateral_at(points, forward, shift).or_else(|| offset.map(|offset| offset * width))
}

#[must_use]
pub fn radar_point_road_edge_outside_distance_m(
    point: &RadarPoint,
    state: &ClusterUiState,
    width: f64,
) -> Option<f64> {
    let left = road_edge_lateral_at(
        &state.left_road_edge_points,
        state.left_road_edge_lateral_shift_m,
        state.left_road_edge_offset,
        point.longitudinal_m,
        width,
    );
    let right = road_edge_lateral_at(
        &state.right_road_edge_points,
        state.right_road_edge_lateral_shift_m,
        state.right_road_edge_offset,
        point.longitudinal_m,
        width,
    );
    if left.zip(right).is_some_and(|(left, right)| left >= right) {
        return None;
    }
    let mut outside: f64 = 0.0;
    if let Some(left) = left.filter(|left| point.lateral_m < *left) {
        outside = outside.max(left - point.lateral_m);
    }
    if let Some(right) = right.filter(|right| point.lateral_m > *right) {
        outside = outside.max(point.lateral_m - right);
    }
    Some(outside)
}

#[must_use]
pub fn radar_point_road_edge_distance_m(
    point: &RadarPoint,
    state: &ClusterUiState,
    width: f64,
) -> Option<f64> {
    let mut distances = Vec::new();
    for (points, shift) in [
        (
            &state.left_road_edge_points,
            state.left_road_edge_lateral_shift_m,
        ),
        (
            &state.right_road_edge_points,
            state.right_road_edge_lateral_shift_m,
        ),
    ] {
        if let Some(lateral) = model_line_lateral_at(points, point.longitudinal_m, shift) {
            distances.push((point.lateral_m - lateral).abs());
        }
    }
    for offset in [state.left_road_edge_offset, state.right_road_edge_offset]
        .into_iter()
        .flatten()
    {
        distances.push((point.lateral_m - offset * width).abs());
    }
    distances
        .into_iter()
        .reduce(|left, right| if right < left { right } else { left })
}

#[must_use]
pub fn radar_point_is_outside_road_edges(
    point: &RadarPoint,
    state: &ClusterUiState,
    width: f64,
) -> bool {
    radar_point_road_edge_outside_distance_m(point, state, width)
        .is_some_and(|distance| distance > 0.0)
}

#[must_use]
pub fn radar_point_is_near_or_outside_road_edge(
    point: &RadarPoint,
    state: &ClusterUiState,
    width: f64,
    outside: Option<f64>,
) -> bool {
    outside.is_some_and(|distance| distance > 0.0)
        || radar_point_road_edge_distance_m(point, state, width)
            .is_some_and(|distance| distance <= 1.05)
}

#[must_use]
pub fn radar_point_matches_static_road_edge(
    point: &RadarPoint,
    state: &ClusterUiState,
    width: f64,
) -> bool {
    let Some(distance) = radar_point_road_edge_distance_m(point, state, width) else {
        return false;
    };
    distance <= 0.50
        || (distance <= 1.05
            && (radar_point_absolute_speed_kph(point, state)
                .is_some_and(|speed| speed.abs() <= 8.0)
                || point.relative_speed_mps.unwrap_or(0.0).abs() <= 1.25))
}

#[must_use]
pub fn radar_point_is_outside_outer_lane(
    point: &RadarPoint,
    state: &ClusterUiState,
    width: f64,
) -> bool {
    let laterals: Vec<_> = state
        .lanes
        .iter()
        .filter(|marking| marking.visible)
        .map(|marking| {
            model_line_lateral_at(
                &marking.model_points,
                point.longitudinal_m,
                marking.model_lateral_shift_m,
            )
            .unwrap_or(marking.offset * width)
        })
        .collect();
    if laterals.len() < 2 {
        return false;
    }
    let low = laterals[1..].iter().fold(
        laterals[0],
        |left, &right| if right < left { right } else { left },
    );
    let high = laterals[1..].iter().fold(
        laterals[0],
        |left, &right| if right > left { right } else { left },
    );
    point.lateral_m <= low || point.lateral_m >= high
}
