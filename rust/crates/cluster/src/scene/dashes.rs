use super::{
    geometry::{extend_model_centerline_rearward, lane_centerline, LaneSpan},
    lines::model_line_centerline,
    math::{append_unique_point, lerp_vec3},
    Vec3,
};
use crate::{config::EGO_FORWARD_M, models::LaneMarking};

pub const LANE_DASH_LENGTH_M: f64 = 5.2;
pub const LANE_DASH_GAP_M: f64 = 4.2;
pub const DRIVE_VIEW_REAR_VISIBLE_M: f64 = EGO_FORWARD_M - 5.0;

#[must_use]
pub fn lane_dash_cycle_offset(distance: f64, phase: f64, cycle: f64, dash: f64, eps: f64) -> f64 {
    let offset = (distance + phase).rem_euclid(cycle);
    if offset < eps || (offset - cycle).abs() < eps {
        0.0
    } else if (offset - dash).abs() < eps {
        dash
    } else {
        offset
    }
}

#[must_use]
pub fn lane_dash_phase_m(points: &[Vec3], dash: f64, gap: f64) -> f64 {
    points.first().map_or(0.0, |point| {
        (-0.0_f64.max(DRIVE_VIEW_REAR_VISIBLE_M - point.y)).rem_euclid(dash + gap)
    })
}

#[must_use]
pub fn dashed_lane_start_cursor_m(start: f64, dash: f64, gap: f64) -> f64 {
    if start >= DRIVE_VIEW_REAR_VISIBLE_M {
        return start;
    }
    let cycle = dash + gap;
    DRIVE_VIEW_REAR_VISIBLE_M - ((DRIVE_VIEW_REAR_VISIBLE_M - start) / cycle).ceil() * cycle
}

#[must_use]
pub fn dashed_centerline_segments(points: &[Vec3], dash: f64, gap: f64) -> Vec<Vec<Vec3>> {
    let Some(mut previous) = points.first().copied().filter(|_| points.len() >= 2) else {
        return Vec::new();
    };
    let cycle = dash + gap;
    let mut segments = Vec::new();
    let mut current_dash = Vec::new();
    let mut distance = 0.0;
    let phase = lane_dash_phase_m(points, dash, gap);
    let eps = 0.0001;
    for &current in &points[1..] {
        let dx = current.x - previous.x;
        let dy = current.y - previous.y;
        let dz = current.z - previous.z;
        let length = (dx * dx + dy * dy + dz * dz).sqrt();
        if length <= 0.001 {
            previous = current;
            continue;
        }
        let end = distance + length;
        let mut cursor = distance;
        let mut cursor_point = previous;
        while cursor < end - eps {
            let offset = lane_dash_cycle_offset(cursor, phase, cycle, dash, eps);
            let in_dash = offset < dash;
            let boundary = cursor
                + if in_dash {
                    dash - offset
                } else {
                    cycle - offset
                };
            let mut next = end.min(boundary);
            if next <= cursor + eps {
                next = end;
            }
            let next_point = lerp_vec3(previous, current, (next - distance) / length);
            if in_dash {
                append_unique_point(&mut current_dash, cursor_point);
                append_unique_point(&mut current_dash, next_point);
                if boundary <= next + eps && current_dash.len() >= 2 {
                    segments.push(std::mem::take(&mut current_dash));
                }
            } else if current_dash.len() >= 2 {
                segments.push(std::mem::take(&mut current_dash));
            }
            cursor = next;
            cursor_point = next_point;
        }
        distance = end;
        previous = current;
    }
    if current_dash.len() >= 2 {
        segments.push(current_dash);
    }
    segments
}

#[must_use]
pub fn lane_marking_segments_for_marking(
    marking: &LaneMarking,
    span: &LaneSpan,
    extend_before_model: bool,
    scale: f64,
) -> Vec<Vec<Vec3>> {
    if !marking.model_points.is_empty() {
        let mut centerline = model_line_centerline(
            &marking.model_points,
            span.start,
            span.end,
            0.0,
            marking.model_lateral_shift_m,
            scale,
        );
        if centerline.len() < 2 {
            if !extend_before_model {
                return Vec::new();
            }
        } else {
            if extend_before_model {
                centerline = extend_model_centerline_rearward(&centerline, span.start);
            }
            return if marking.style == "solid" {
                vec![centerline]
            } else {
                dashed_centerline_segments(&centerline, LANE_DASH_LENGTH_M, LANE_DASH_GAP_M)
            };
        }
    }
    if marking.style == "solid" {
        return vec![lane_centerline(
            marking.offset,
            span.steering,
            span.lane_width,
            span.start,
            span.end,
            56,
            0.0,
        )];
    }
    let mut segments = Vec::new();
    let cycle = LANE_DASH_LENGTH_M + LANE_DASH_GAP_M;
    let mut cursor = dashed_lane_start_cursor_m(span.start, LANE_DASH_LENGTH_M, LANE_DASH_GAP_M);
    while cursor < span.end {
        let start = cursor.max(span.start);
        let end = (cursor + LANE_DASH_LENGTH_M).min(span.end);
        if end > start + 0.001 {
            segments.push(lane_centerline(
                marking.offset,
                span.steering,
                span.lane_width,
                start,
                end,
                6,
                0.0,
            ));
        }
        cursor += cycle;
    }
    segments
}
