use super::{
    math::{clamp, normalize2, sample_range},
    Color, MeshStrip, Vec3,
};
use crate::config::{ROAD_CURVE_M_PER_M2, VEHICLE_LANE_CHANGE_SLOPE};

#[must_use]
pub fn road_curve_m(forward: f64, steering: f64) -> f64 {
    clamp(steering, -1.0, 1.0) * ROAD_CURVE_M_PER_M2 * forward * forward
}

#[must_use]
pub fn road_world_x(offset: f64, forward: f64, steering: f64, lane_width: f64) -> f64 {
    offset * lane_width + road_curve_m(forward, steering)
}

#[must_use]
pub fn vehicle_heading(
    offset: f64,
    forward: f64,
    steering: f64,
    lane_width: f64,
    target_offset: Option<f64>,
) -> (f64, f64, f64, f64) {
    let road_slope = 2.0 * clamp(steering, -1.0, 1.0) * ROAD_CURVE_M_PER_M2 * forward;
    let lane_change_slope = target_offset.map_or(0.0, |target| {
        clamp(
            (target - offset) * lane_width / 18.0,
            -VEHICLE_LANE_CHANGE_SLOPE,
            VEHICLE_LANE_CHANGE_SLOPE,
        )
    });
    let (forward_x, forward_y) = normalize2(road_slope + lane_change_slope, 1.0);
    (forward_y, -forward_x, forward_x, forward_y)
}

#[must_use]
pub fn lane_centerline(
    offset: f64,
    steering: f64,
    lane_width: f64,
    start: f64,
    end: f64,
    steps: u32,
    height: f64,
) -> Vec<Vec3> {
    sample_range(start, end, steps)
        .into_iter()
        .map(|forward| {
            Vec3::new(
                road_world_x(offset, forward, steering, lane_width),
                forward,
                height,
            )
        })
        .collect()
}

pub struct LaneSpan {
    pub steering: f64,
    pub lane_width: f64,
    pub start: f64,
    pub end: f64,
    pub steps: u32,
}

#[must_use]
pub fn strip_between_offsets(
    left: f64,
    right: f64,
    span: &LaneSpan,
    color: Color,
    height: f64,
) -> MeshStrip {
    MeshStrip::new(
        lane_centerline(
            left,
            span.steering,
            span.lane_width,
            span.start,
            span.end,
            span.steps,
            height,
        ),
        lane_centerline(
            right,
            span.steering,
            span.lane_width,
            span.start,
            span.end,
            span.steps,
            height,
        ),
        color,
    )
}

#[must_use]
pub fn strip_from_centerline(points: &[Vec3], width: f64, color: Color) -> MeshStrip {
    if points.len() < 2 {
        return MeshStrip::new(points, points, color);
    }
    let mut left = Vec::with_capacity(points.len());
    let mut right = Vec::with_capacity(points.len());
    let half_width = width * 0.5;
    for (index, point) in points.iter().enumerate() {
        let previous = points[index.saturating_sub(1)];
        let next = points[(index + 1).min(points.len() - 1)];
        let (tangent_x, tangent_y) = normalize2(next.x - previous.x, next.y - previous.y);
        left.push(Vec3::new(
            point.x - tangent_y * half_width,
            point.y + tangent_x * half_width,
            point.z,
        ));
        right.push(Vec3::new(
            point.x + tangent_y * half_width,
            point.y - tangent_x * half_width,
            point.z,
        ));
    }
    MeshStrip::new(left, right, color)
}

#[must_use]
pub fn extend_centerline_rearward_to_first_point(
    points: &[Vec3],
    start: f64,
    height: f64,
) -> Vec<Vec3> {
    if points.len() < 2 || start >= points[0].y - 0.10 {
        return points.to_vec();
    }
    let first = points[0];
    let length = first.y - start;
    let mut result = vec![Vec3::new(first.x, start, height)];
    let transition = first.y - 1.0_f64.min(length * 0.25);
    if transition > start + 0.10 {
        result.push(Vec3::new(first.x, transition, height));
    }
    result.extend_from_slice(points);
    result
}

#[must_use]
pub fn extend_model_centerline_rearward(points: &[Vec3], start: f64) -> Vec<Vec3> {
    extend_centerline_rearward_to_first_point(points, start, 0.0)
}
