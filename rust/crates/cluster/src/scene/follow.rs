use super::{lines::render_scene_forward_m, math::clamp, Color, MeshStrip, Vec3};
use crate::{
    config::PATH_HEIGHT_M,
    models::{ClusterUiState, CruiseDisplayState},
};

#[must_use]
pub fn cruise_follow_distance_marker_m(state: &ClusterUiState) -> Option<f64> {
    if state.cruise_display_state == CruiseDisplayState::Off {
        return None;
    }
    let time = state.longitudinal_t_follow_s.or_else(|| {
        state.cruise_gap.map(|gap| match gap.clamp(1, 4) {
            1 => 1.25,
            2 => 1.45,
            3 => 1.75,
            _ => 2.0,
        })
    })?;
    if !time.is_finite() || time <= 0.0 {
        return None;
    }
    Some(6.0 + 0.0_f64.max(state.speed_kph / 3.6) * time)
}

#[must_use]
pub fn centerline_x_at_forward(points: &[Vec3], forward: f64) -> Option<f64> {
    let mut previous = points.first()?;
    if forward <= previous.y {
        return Some(previous.x);
    }
    for point in &points[1..] {
        if forward <= point.y {
            let amount = clamp(
                (forward - previous.y) / 0.001_f64.max(point.y - previous.y),
                0.0,
                1.0,
            );
            return Some(previous.x + (point.x - previous.x) * amount);
        }
        previous = point;
    }
    points.last().map(|point| point.x)
}

fn marker_strip(
    center: f64,
    forward: f64,
    width: f64,
    half_forward: f64,
    height: f64,
    color: Color,
) -> MeshStrip {
    let radius = (width * 0.5).min(half_forward);
    let straight_half = width * 0.5 - radius;
    let mut left = Vec::with_capacity(11);
    let mut right = Vec::with_capacity(11);
    for index in 0..=10 {
        let offset = -half_forward + 2.0 * half_forward * f64::from(index) / 10.0;
        let cap = 0.0_f64.max(radius * radius - offset * offset).sqrt();
        let half_width = straight_half + cap;
        left.push(Vec3::new(center - half_width, forward + offset, height));
        right.push(Vec3::new(center + half_width, forward + offset, height));
    }
    MeshStrip::new(left, right, color)
}

#[must_use]
pub fn follow_distance_marker_strips(state: &ClusterUiState, points: &[Vec3]) -> Vec<MeshStrip> {
    let Some(distance) =
        cruise_follow_distance_marker_m(state).filter(|value| *value > 0.0 && points.len() >= 2)
    else {
        return Vec::new();
    };
    let forward = render_scene_forward_m(distance, state);
    if forward < points[0].y || forward > points[points.len() - 1].y {
        return Vec::new();
    }
    let Some(center) = centerline_x_at_forward(points, forward) else {
        return Vec::new();
    };
    vec![
        marker_strip(
            center,
            forward,
            1.8 + 0.07 * 2.0,
            0.26 + 0.07,
            PATH_HEIGHT_M + 0.116,
            [5, 9, 12, 230],
        ),
        marker_strip(
            center,
            forward,
            1.8,
            0.26,
            PATH_HEIGHT_M + 0.116 + 0.001,
            [255, 255, 255, 242],
        ),
    ]
}
