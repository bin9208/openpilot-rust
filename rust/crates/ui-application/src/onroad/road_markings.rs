//! Shared lane-dash and blindspot geometry from ui/road_markings.py (MIT).
use super::model_renderer::{
    math::{float, interp},
    points::ModelPoint,
    projection::{ribbon_vertices, Projection},
};
use crate::Error;
use openpilot_ui_framework::geometry::Point;

pub const DASH_LENGTH: f64 = 5.2;
pub const DASH_GAP: f64 = 4.2;

pub fn lane_dash_segments(
    line: &[ModelPoint],
    max_distance: f64,
) -> Result<Vec<Vec<ModelPoint>>, Error> {
    if line.len() < 2 {
        return Ok(Vec::new());
    }
    let start = f64::from(line[0].0[0]).max(0.);
    let end = max_distance.min(f64::from(line[line.len() - 1].0[0]));
    if end <= start {
        return Ok(Vec::new());
    }
    let cycle = DASH_LENGTH + DASH_GAP;
    let mut cursor = (start / cycle).floor() * cycle;
    let x: Vec<_> = line.iter().map(|p| f64::from(p.0[0])).collect();
    let y: Vec<_> = line.iter().map(|p| f64::from(p.0[1])).collect();
    let z: Vec<_> = line.iter().map(|p| f64::from(p.0[2])).collect();
    let endpoint = |distance| -> Result<ModelPoint, Error> {
        Ok(ModelPoint([
            float(distance),
            float(interp(distance, &x, &y)?),
            float(interp(distance, &x, &z)?),
        ]))
    };
    let mut segments = Vec::new();
    while cursor < end {
        let a = cursor.max(start);
        let b = (cursor + DASH_LENGTH).min(end);
        if b > a {
            let mut points = Vec::with_capacity(line.len() + 2);
            points.push(endpoint(a)?);
            points.extend(
                line.iter()
                    .copied()
                    .filter(|p| f64::from(p.0[0]) > a && f64::from(p.0[0]) < b),
            );
            points.push(endpoint(b)?);
            segments.push(points);
        }
        cursor += cycle;
    }
    Ok(segments)
}
pub fn project_lane_segments(
    projection: &Projection,
    segments: &[Vec<ModelPoint>],
    half_width: f64,
) -> Vec<Vec<Point>> {
    let offsets = [float(-half_width), float(half_width)];
    let mut result = Vec::with_capacity(segments.len());
    for segment in segments {
        let pairs: Vec<_> = segment
            .iter()
            .filter(|p| p.0[0] >= 0.)
            .filter_map(|p| {
                let sides = offsets
                    .map(|y| projection.batch_float(ModelPoint([p.0[0], p.0[1] + y, p.0[2]])));
                projection.pair_float(sides[0], sides[1])
            })
            .collect();
        if !pairs.is_empty() {
            result.push(ribbon_vertices(&pairs));
        }
    }
    result
}
pub fn project_blindspot_barrier(
    projection: &Projection,
    line: &[ModelPoint],
    shift: f64,
) -> Vec<Point> {
    let shift = float(shift);
    let mut lowest = f32::INFINITY;
    let mut pairs = Vec::with_capacity(line.len());
    for p in line.iter().filter(|p| p.0[0] >= 0.) {
        let sides = [1.15_f32, 0.6]
            .map(|z| projection.explicit_float(ModelPoint([p.0[0], p.0[1] + shift, p.0[2] + z])));
        if let Some(pair) = projection.pair_float(sides[0], sides[1]) {
            if pair[0].y <= lowest {
                lowest = pair[0].y;
                pairs.push(pair);
            }
        }
    }
    ribbon_vertices(&pairs)
}
pub fn blindspot_barrier_quads(points: &[Point]) -> Vec<[Point; 4]> {
    let count = points.len();
    let half = count / 2;
    if half < 3 {
        return Vec::new();
    }
    (0..half - 2)
        .step_by(2)
        .map(|start| {
            let mut quad = [
                points[start],
                points[start + 1],
                points[count - start - 3],
                points[count - start - 2],
            ];
            let cx = quad.iter().fold(0_f32, |sum, p| sum + p.x) / 4.;
            let cy = quad.iter().fold(0_f32, |sum, p| sum + p.y) / 4.;
            quad.sort_by(|a, b| {
                (a.y - cy)
                    .atan2(a.x - cx)
                    .total_cmp(&(b.y - cy).atan2(b.x - cx))
            });
            quad
        })
        .collect()
}
