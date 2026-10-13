use crate::{
    models::{radar_position_is_zero, RadarPoint},
    scene::math::clamp,
};
use std::cmp::Ordering;

#[must_use]
pub fn sorted_points(points: impl IntoIterator<Item = RadarPoint>) -> Vec<RadarPoint> {
    let mut points: Vec<_> = points
        .into_iter()
        .filter(|point| {
            (-12.0..=12.0).contains(&point.lateral_m)
                && (0.0..=180.0).contains(&point.longitudinal_m)
                && !radar_position_is_zero(point.longitudinal_m, point.lateral_m)
        })
        .collect();
    points.sort_by(|left, right| {
        left.longitudinal_m
            .partial_cmp(&right.longitudinal_m)
            .unwrap_or(Ordering::Equal)
            .then_with(|| {
                left.lateral_m
                    .abs()
                    .partial_cmp(&right.lateral_m.abs())
                    .unwrap_or(Ordering::Equal)
            })
            .then_with(|| left.label.cmp(&right.label))
    });
    points
}
#[must_use]
pub fn adjust_point(
    point: &RadarPoint,
    yaw: Option<f64>,
    gain: f64,
    offset: f64,
    width: f64,
) -> RadarPoint {
    let mut point = point.clone();
    if point.source != "cornerRadar"
        || !(point.label.starts_with("CO") || point.label.starts_with("CR"))
    {
        return point;
    }
    if gain != 0.0 {
        if let Some(yaw) = yaw.filter(|yaw| yaw.is_finite() && yaw.abs() < 0.35) {
            let correction = clamp(
                yaw * clamp(point.longitudinal_m, 0.0, 50.0) * gain,
                -1.5,
                1.5,
            );
            point.lateral_speed_mps = point.lateral_speed_mps.map(|speed| speed + correction);
        }
    }
    let minimum = (clamp(width, 2.4, 4.6) * 0.55).max(1.8);
    if offset != 0.0 && point.lateral_m.abs() >= minimum {
        point.lateral_m -= offset.abs().copysign(point.lateral_m);
    }
    point
}
