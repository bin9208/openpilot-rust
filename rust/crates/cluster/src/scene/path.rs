use super::profile::{record, ProfileAdd};
use super::{
    geometry::{road_world_x, strip_from_centerline},
    geometry_cache::GeometryCache,
    lines::{
        longitudinal_render_distance_scale, model_line_lateral_at_forward,
        render_relative_forward_m_from_scene, render_scene_forward_m,
        render_scene_forward_m_for_scale,
    },
    math::{clamp, sample_range, smoothstep},
    Color, MeshStrip, PathBlocker, Vec3,
};
use crate::{
    config::{
        ClusterTheme, AMBER, EGO_FORWARD_M, PATH_END_M, PATH_HEIGHT_M,
        PATH_LANE_CHANGE_CURVE_END_M, PATH_LANE_CHANGE_CURVE_START_M, PATH_START_M, RED,
    },
    models::ClusterUiState,
};
use num_traits::{AsPrimitive, ToPrimitive};
use std::time::Instant;

#[must_use]
pub fn planned_path_lane_offset(state: &ClusterUiState, forward: f64) -> f64 {
    if state.lane_change.is_none() {
        return 0.0;
    }
    let start = clamp(state.ego_lane_offset, -1.25, 1.25);
    let target = state.highlight_lane_offset.unwrap_or(0.0);
    let blend = smoothstep(
        (forward - PATH_LANE_CHANGE_CURVE_START_M)
            / (PATH_LANE_CHANGE_CURVE_END_M - PATH_LANE_CHANGE_CURVE_START_M),
    );
    start + (target - start) * blend
}

#[must_use]
pub fn planned_path_end_m(state: &ClusterUiState, blockers: &[PathBlocker]) -> f64 {
    let mut end = PATH_END_M;
    for blocker in blockers {
        if blocker.forward_m <= PATH_START_M
            || (planned_path_lane_offset(state, blocker.forward_m) - blocker.offset).abs() > 0.42
        {
            continue;
        }
        let stop = blocker.forward_m - blocker.length_m * 0.5 - 0.25;
        end = end.min((PATH_START_M + 0.6).max(stop));
    }
    end
}

#[must_use]
pub fn model_path_lateral_at_forward(state: &ClusterUiState, relative: f64) -> Option<f64> {
    model_line_lateral_at_forward(&state.model_path, relative, 0.0)
}

#[must_use]
pub fn model_path_world_x(state: &ClusterUiState, lane_width: f64, forward: f64) -> Option<f64> {
    let relative =
        render_relative_forward_m_from_scene(forward, longitudinal_render_distance_scale(state));
    let lateral = model_path_lateral_at_forward(state, relative)?;
    Some(
        road_world_x(
            clamp(state.ego_lane_offset, -1.25, 1.25),
            EGO_FORWARD_M,
            state.steering,
            lane_width,
        ) + lateral,
    )
}

#[must_use]
pub fn model_path_end_m(
    state: &ClusterUiState,
    lane_width: f64,
    blockers: &[PathBlocker],
) -> Option<f64> {
    if state.model_path.len() < 2 {
        return None;
    }
    let mut end = PATH_END_M.min(render_scene_forward_m(
        state.model_path.last()?.forward_m,
        state,
    ));
    if end <= PATH_START_M + 0.6 {
        return None;
    }
    for blocker in blockers {
        if blocker.forward_m <= PATH_START_M {
            continue;
        }
        let Some(path_x) = model_path_world_x(state, lane_width, blocker.forward_m) else {
            continue;
        };
        let blocker_x = road_world_x(
            blocker.offset,
            blocker.forward_m,
            state.steering,
            lane_width,
        );
        if (path_x - blocker_x).abs() > 0.42 * lane_width {
            continue;
        }
        let stop = blocker.forward_m - blocker.length_m * 0.5 - 0.25;
        end = end.min((PATH_START_M + 0.6).max(stop));
    }
    Some(end)
}

fn fallback_steps(end: f64) -> u32 {
    (32.0 * (end - PATH_START_M) / (PATH_END_M - PATH_START_M))
        .max(4.0)
        .to_u32()
        .expect("bounded path sampling count")
}

#[must_use]
pub fn model_path_centerline(
    state: &ClusterUiState,
    lane_width: f64,
    blockers: &[PathBlocker],
) -> Vec<Vec3> {
    let Some(end) = model_path_end_m(state, lane_width, blockers) else {
        return Vec::new();
    };
    let scale = longitudinal_render_distance_scale(state);
    let relative_start = 0.0_f64.max(render_relative_forward_m_from_scene(PATH_START_M, scale));
    let relative_end = relative_start.max(render_relative_forward_m_from_scene(end, scale));
    let model_points: Vec<_> = state
        .model_path
        .iter()
        .filter(|point| relative_start <= point.forward_m && point.forward_m <= relative_end)
        .collect();
    let points = if model_points.len() < 2 {
        sample_range(PATH_START_M, end, fallback_steps(end))
            .into_iter()
            .filter_map(|forward| {
                model_path_world_x(state, lane_width, forward)
                    .map(|x| Vec3::new(x, forward, PATH_HEIGHT_M))
            })
            .collect::<Vec<_>>()
    } else {
        let ego_x = road_world_x(
            clamp(state.ego_lane_offset, -1.25, 1.25),
            EGO_FORWARD_M,
            state.steering,
            lane_width,
        );
        model_points
            .iter()
            .map(|point| {
                Vec3::new(
                    ego_x + point.lateral_m,
                    render_scene_forward_m_for_scale(point.forward_m, scale),
                    PATH_HEIGHT_M,
                )
            })
            .collect()
    };
    if points.len() >= 2 {
        points
    } else {
        Vec::new()
    }
}

#[must_use]
pub fn model_path_uncertainty_width(state: &ClusterUiState) -> Option<f64> {
    let values: Vec<_> = state
        .model_path
        .iter()
        .filter_map(|point| point.lateral_std_m)
        .take(16)
        .collect();
    if values.is_empty() {
        return None;
    }
    let count: f64 = values.len().as_();
    let average = values.iter().sum::<f64>() / count;
    Some(clamp(0.68 + average * 0.42, 0.72, 1.85))
}

#[must_use]
pub fn path_metric_color(acceleration: f64) -> Color {
    if acceleration <= -2.4 {
        [RED[0], RED[1], RED[2], 210]
    } else if acceleration <= -0.7 {
        [AMBER[0], AMBER[1], AMBER[2], 190]
    } else if acceleration >= 0.7 {
        [18, 184, 108, 170]
    } else {
        [70, 152, 255, 145]
    }
}

#[must_use]
pub fn model_path_metric_strips(state: &ClusterUiState, points: &[Vec3]) -> Vec<MeshStrip> {
    let count = state.model_path.len().min(points.len());
    if count < 2 {
        return Vec::new();
    }
    let segments = (count - 1).min(14);
    let mut strips = Vec::new();
    for segment in 0..segments {
        let numerator: f64 = (segment * (count - 1)).as_();
        let denominator: f64 = segments.saturating_sub(1).max(1).as_();
        let position = numerator / denominator;
        let index = position
            .round_ties_even()
            .to_usize()
            .unwrap_or(usize::MAX)
            .min(points.len() - 2);
        let numerator: f64 = (index * (state.model_path.len() - 1)).as_();
        let denominator: f64 = (count - 1).max(1).as_();
        let model_position = numerator / denominator;
        let model_index = model_position
            .round_ties_even()
            .to_usize()
            .unwrap_or(usize::MAX)
            .min(state.model_path.len() - 1);
        let Some(acceleration) = state.model_path[model_index].accel_mps2 else {
            continue;
        };
        let height = PATH_HEIGHT_M + 0.066;
        let pair = [
            Vec3::new(points[index].x, points[index].y, height),
            Vec3::new(points[index + 1].x, points[index + 1].y, height),
        ];
        strips.push(strip_from_centerline(
            &pair,
            0.24,
            path_metric_color(acceleration),
        ));
    }
    strips
}

#[must_use]
pub fn planned_path_strips(
    cache: &mut GeometryCache,
    state: &ClusterUiState,
    lane_width: f64,
    blockers: &[PathBlocker],
    theme: &ClusterTheme,
    profile: &mut ProfileAdd<'_>,
) -> Vec<MeshStrip> {
    let started = Instant::now();
    let mut points = model_path_centerline(state, lane_width, blockers);
    let model_driven = !points.is_empty();
    if points.is_empty() {
        let end = planned_path_end_m(state, blockers);
        points = sample_range(PATH_START_M, end, fallback_steps(end))
            .into_iter()
            .map(|forward| {
                Vec3::new(
                    road_world_x(
                        planned_path_lane_offset(state, forward),
                        forward,
                        state.steering,
                        lane_width,
                    ),
                    forward,
                    PATH_HEIGHT_M,
                )
            })
            .collect();
    }
    record(profile, "scene.build.planned_path.centerline", started);
    let started = Instant::now();
    let mut specs = vec![(0.86, theme.path_shadow, 0.024)];
    if model_driven {
        if let Some(width) = model_path_uncertainty_width(state) {
            specs.push((width, theme.path_uncertainty, PATH_HEIGHT_M + 0.002));
        }
    }
    specs.extend([
        (0.46, theme.path_body, PATH_HEIGHT_M + 0.046),
        (0.16, theme.path_highlight, PATH_HEIGHT_M + 0.088),
    ]);
    let mut strips = cache.planned_strips(&points, &specs);
    record(profile, "scene.build.planned_path.strips", started);
    if model_driven {
        let highlight = strips.pop();
        let started = Instant::now();
        strips.extend(model_path_metric_strips(state, &points));
        record(profile, "scene.build.planned_path.metrics", started);
        if let Some(highlight) = highlight {
            strips.push(highlight);
        }
    }
    let started = Instant::now();
    let marker = if blockers.is_empty() {
        Vec::new()
    } else {
        model_path_centerline(state, lane_width, &[])
    };
    strips.extend(super::follow::follow_distance_marker_strips(
        state,
        if marker.is_empty() { &points } else { &marker },
    ));
    record(profile, "scene.build.planned_path.follow_distance", started);
    strips
}
