use super::{
    math::clamp,
    radar_matching::radar_point_matches_detected_vehicle,
    radar_motion::{
        radar_point_absolute_speed_kph, radar_point_has_meaningful_motion,
        radar_point_is_cross_traffic, radar_point_is_oncoming,
    },
};
use crate::{
    config::CLUSTER_RADAR_DISPLAY_DETAIL,
    models::{radar_position_is_zero, ClusterUiState, RadarPoint},
};
use num_traits::ToPrimitive;
use std::cmp::Ordering;

#[must_use]
pub fn radar_point_display_cmp(
    left: &RadarPoint,
    right: &RadarPoint,
    state: &ClusterUiState,
) -> Ordering {
    let flags = |point: &RadarPoint| {
        [
            !radar_point_matches_detected_vehicle(point, state),
            !(radar_point_is_oncoming(point, state) || radar_point_is_cross_traffic(point, 0.0)),
            !point.promotion_held,
            point.in_my_lane.is_none_or(|value| value <= 0),
            !radar_point_has_meaningful_motion(point, state),
            point.valid.is_some_and(|value| value <= 0),
        ]
    };
    flags(left)
        .cmp(&flags(right))
        .then_with(|| {
            left.longitudinal_m
                .abs()
                .partial_cmp(&right.longitudinal_m.abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .then_with(|| {
            left.lateral_m
                .abs()
                .partial_cmp(&right.lateral_m.abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .then_with(|| {
            right
                .valid_count
                .unwrap_or(0)
                .cmp(&left.valid_count.unwrap_or(0))
        })
        .then_with(|| {
            right
                .probability
                .unwrap_or(0.0)
                .partial_cmp(&left.probability.unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .then_with(|| left.label.cmp(&right.label))
}

#[must_use]
pub fn radar_points_are_mergeable(
    left: &RadarPoint,
    right: &RadarPoint,
    state: &ClusterUiState,
) -> bool {
    let tolerance = clamp(
        left.longitudinal_m.abs().max(right.longitudinal_m.abs()) * 0.018,
        0.75,
        2.4,
    );
    (left.longitudinal_m - right.longitudinal_m).abs() <= tolerance
        && (left.lateral_m - right.lateral_m).abs() <= 0.65
        && !radar_point_absolute_speed_kph(left, state)
            .zip(radar_point_absolute_speed_kph(right, state))
            .is_some_and(|(left, right)| (left - right).abs() > 3.0)
}

fn average(values: impl Iterator<Item = f64>) -> Option<f64> {
    let (sum, count) = values.fold((0.0, 0_usize), |(sum, count), value| {
        (sum + value, count + 1)
    });
    (count > 0).then(|| sum / count.to_f64().expect("bounded radar point count"))
}

#[must_use]
pub fn merged_radar_point(points: &[RadarPoint], state: &ClusterUiState) -> Option<RadarPoint> {
    let first = points.first()?;
    let mut merged = first.clone();
    if points.len() > 1 {
        merged.label = format!("{}+{}", first.label, points.len() - 1);
    }
    if points.iter().any(|point| point.source != first.source) {
        merged.source = "merged".into();
    }
    merged.longitudinal_m = average(points.iter().map(|point| point.longitudinal_m)).unwrap_or(0.0);
    merged.lateral_m = average(points.iter().map(|point| point.lateral_m)).unwrap_or(0.0);
    merged.relative_speed_mps = average(points.iter().filter_map(|point| point.relative_speed_mps));
    merged.absolute_speed_kph = average(
        points
            .iter()
            .filter_map(|point| radar_point_absolute_speed_kph(point, state)),
    );
    merged.lateral_speed_mps = average(points.iter().filter_map(|point| point.lateral_speed_mps));
    merged.relative_accel_mps2 =
        average(points.iter().filter_map(|point| point.relative_accel_mps2));
    merged.probability = average(points.iter().filter_map(|point| point.probability));
    merged.valid = points.iter().filter_map(|point| point.valid).max();
    merged.valid_count = points.iter().filter_map(|point| point.valid_count).max();
    merged.in_my_lane = points.iter().filter_map(|point| point.in_my_lane).max();
    merged.motion_consistent = points
        .iter()
        .filter_map(|point| point.motion_consistent)
        .reduce(|left, right| left && right);
    merged.promotion_held = points.iter().any(|point| point.promotion_held);
    Some(merged)
}

#[must_use]
pub fn merged_radar_points(points: &[RadarPoint], state: &ClusterUiState) -> Vec<RadarPoint> {
    if points.len() < 2 {
        return points.to_vec();
    }
    let mut ordered: Vec<_> = points.iter().collect();
    ordered.sort_by(|left, right| {
        left.longitudinal_m
            .partial_cmp(&right.longitudinal_m)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                left.lateral_m
                    .partial_cmp(&right.lateral_m)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| left.label.cmp(&right.label))
    });
    let mut groups: Vec<(Vec<RadarPoint>, RadarPoint)> = Vec::new();
    for point in ordered {
        if let Some((group, centroid)) = groups
            .iter_mut()
            .find(|(_, centroid)| radar_points_are_mergeable(point, centroid, state))
        {
            group.push(point.clone());
            if let Some(merged) = merged_radar_point(group, state) {
                *centroid = merged;
            }
        } else {
            groups.push((vec![point.clone()], point.clone()));
        }
    }
    let mut merged: Vec<_> = groups.into_iter().map(|(_, centroid)| centroid).collect();
    sort_by_distance(&mut merged);
    merged
}

fn sort_by_distance(points: &mut [RadarPoint]) {
    points.sort_by(|left, right| {
        left.longitudinal_m
            .partial_cmp(&right.longitudinal_m)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                left.lateral_m
                    .abs()
                    .partial_cmp(&right.lateral_m.abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| left.label.cmp(&right.label))
    });
}

#[must_use]
pub fn radar_points_for_display(state: &ClusterUiState) -> Vec<RadarPoint> {
    let points: Vec<_> = state
        .radar_points
        .iter()
        .filter(|point| !radar_position_is_zero(point.longitudinal_m, point.lateral_m))
        .cloned()
        .collect();
    if state.radar_display_mode == CLUSTER_RADAR_DISPLAY_DETAIL {
        points
    } else {
        merged_radar_points(&points, state)
    }
}

#[must_use]
pub fn raw_corner_radar_points(points: &[RadarPoint]) -> Vec<RadarPoint> {
    let mut corners: Vec<_> = points
        .iter()
        .filter(|point| point.source == "cornerRadar")
        .cloned()
        .collect();
    sort_by_distance(&mut corners);
    corners
}

#[must_use]
pub fn corner_radar_points_for_cluster_display(
    points: &[RadarPoint],
    state: &ClusterUiState,
) -> Vec<RadarPoint> {
    let mut bounded = points.to_vec();
    bounded.sort_by(|left, right| radar_point_display_cmp(left, right, state));
    bounded.truncate(64);
    if state.radar_display_mode != CLUSTER_RADAR_DISPLAY_DETAIL {
        bounded = merged_radar_points(&bounded, state);
    }
    bounded.sort_by(|left, right| radar_point_display_cmp(left, right, state));
    bounded.truncate(34);
    bounded
}
