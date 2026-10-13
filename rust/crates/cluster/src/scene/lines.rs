use super::{
    geometry::{strip_between_offsets, LaneSpan},
    math::{clamp, sample_range},
    Color, MeshStrip, Vec3,
};
use crate::{
    config::{cluster_camera_view_is_road_camera, EGO_FORWARD_M, VEHICLE_LENGTH_M},
    models::{ClusterUiState, DetectedVehicle, LaneMarking, ModelPathPoint},
};

#[must_use]
pub fn longitudinal_render_distance_scale(state: &ClusterUiState) -> f64 {
    if cluster_camera_view_is_road_camera(state.camera_view_mode) {
        1.0
    } else {
        0.5
    }
}

#[must_use]
pub fn data_scene_forward_m(relative: f64) -> f64 {
    EGO_FORWARD_M + relative
}

#[must_use]
pub fn render_scene_forward_m(relative: f64, state: &ClusterUiState) -> f64 {
    render_scene_forward_m_for_scale(relative, longitudinal_render_distance_scale(state))
}

#[must_use]
pub fn render_scene_forward_m_for_scale(relative: f64, scale: f64) -> f64 {
    data_scene_forward_m(relative * 0.001_f64.max(scale))
}

#[must_use]
pub fn render_relative_forward_m_from_scene(forward: f64, scale: f64) -> f64 {
    (forward - EGO_FORWARD_M) / 0.001_f64.max(scale)
}

#[must_use]
pub fn scene_data_relative_forward_m(forward: f64) -> f64 {
    forward - EGO_FORWARD_M
}

#[must_use]
pub fn detected_vehicle_scene_forward_m(vehicle: &DetectedVehicle, state: &ClusterUiState) -> f64 {
    let mut forward = render_scene_forward_m(vehicle.longitudinal_m, state);
    if vehicle.longitudinal_m > 0.0 && (vehicle.primary || vehicle.cut_in) {
        forward += VEHICLE_LENGTH_M * 0.5;
    }
    forward
}

#[must_use]
pub fn model_line_lateral_at_forward(
    points: &[ModelPathPoint],
    forward: f64,
    shift: f64,
) -> Option<f64> {
    let mut previous = points.first()?;
    if forward < 0.0 {
        return None;
    }
    if forward <= previous.forward_m {
        return Some(previous.lateral_m + shift);
    }
    for point in &points[1..] {
        if forward <= point.forward_m {
            let span = 0.001_f64.max(point.forward_m - previous.forward_m);
            let amount = clamp((forward - previous.forward_m) / span, 0.0, 1.0);
            return Some(
                previous.lateral_m + (point.lateral_m - previous.lateral_m) * amount + shift,
            );
        }
        previous = point;
    }
    None
}

pub struct ModelLine<'a> {
    pub points: &'a [ModelPathPoint],
    pub shift: f64,
}

pub struct ModelSpan {
    pub start: f64,
    pub end: f64,
    pub steps: u32,
    pub height: f64,
    pub scale: f64,
    pub extend_before_model: bool,
}

#[must_use]
pub fn strip_between_model_lines(
    left: &ModelLine<'_>,
    right: &ModelLine<'_>,
    span: &ModelSpan,
    color: Color,
) -> Option<MeshStrip> {
    if left.points.len() < 2 || right.points.len() < 2 {
        return None;
    }
    let scale = 0.001_f64.max(span.scale);
    let mut relative_start = 0.0_f64.max(render_relative_forward_m_from_scene(span.start, scale));
    if !span.extend_before_model {
        relative_start = relative_start
            .max(left.points[0].forward_m)
            .max(right.points[0].forward_m);
    }
    let relative_end = render_relative_forward_m_from_scene(span.end, scale)
        .min(left.points.last()?.forward_m)
        .min(right.points.last()?.forward_m);
    let start = if span.extend_before_model {
        span.start
    } else {
        render_scene_forward_m_for_scale(relative_start, scale)
    };
    let end = span
        .end
        .min(render_scene_forward_m_for_scale(relative_end, scale));
    if end <= start + 1.0 {
        return None;
    }
    let mut left_vertices = Vec::new();
    let mut right_vertices = Vec::new();
    for forward in sample_range(start, end, span.steps) {
        let relative = render_relative_forward_m_from_scene(forward, scale);
        let interpolate = |line: &ModelLine<'_>| {
            if span.extend_before_model && relative < line.points[0].forward_m {
                Some(line.points[0].lateral_m + line.shift)
            } else {
                model_line_lateral_at_forward(line.points, relative, line.shift)
            }
        };
        if let (Some(left_x), Some(right_x)) = (interpolate(left), interpolate(right)) {
            let (left_x, right_x) = if left_x <= right_x {
                (left_x, right_x)
            } else {
                (right_x, left_x)
            };
            left_vertices.push(Vec3::new(left_x, forward, span.height));
            right_vertices.push(Vec3::new(right_x, forward, span.height));
        }
    }
    (left_vertices.len() >= 2 && right_vertices.len() >= 2)
        .then(|| MeshStrip::new(left_vertices, right_vertices, color))
}

#[must_use]
pub fn marking_near_offset(markings: &[LaneMarking], offset: f64) -> Option<&LaneMarking> {
    let mut closest: Option<&LaneMarking> = None;
    for marking in markings.iter().filter(|marking| marking.visible) {
        if closest.is_none_or(|previous| {
            (marking.offset - offset).abs() < (previous.offset - offset).abs()
        }) {
            closest = Some(marking);
        }
    }
    closest.filter(|marking| (marking.offset - offset).abs() <= 0.30)
}

#[must_use]
pub fn lane_floor_strip(
    state: &ClusterUiState,
    offset: f64,
    color: Color,
    span: &LaneSpan,
    route_mode: bool,
    height: f64,
) -> Option<MeshStrip> {
    if let (Some(left), Some(right)) = (
        marking_near_offset(&state.lanes, offset - 0.5),
        marking_near_offset(&state.lanes, offset + 0.5),
    ) {
        let model = ModelSpan {
            start: span.start,
            end: span.end,
            steps: span.steps,
            height,
            scale: longitudinal_render_distance_scale(state),
            extend_before_model: true,
        };
        if let Some(strip) = strip_between_model_lines(
            &ModelLine {
                points: &left.model_points,
                shift: left.model_lateral_shift_m,
            },
            &ModelLine {
                points: &right.model_points,
                shift: right.model_lateral_shift_m,
            },
            &model,
            color,
        ) {
            return Some(strip);
        }
    }
    if route_mode {
        None
    } else {
        Some(strip_between_offsets(
            offset - 0.5,
            offset + 0.5,
            span,
            color,
            height,
        ))
    }
}

#[must_use]
pub fn model_line_centerline(
    points: &[ModelPathPoint],
    start: f64,
    end: f64,
    height: f64,
    shift: f64,
    scale: f64,
) -> Vec<Vec3> {
    points
        .iter()
        .filter_map(|point| {
            let forward = render_scene_forward_m_for_scale(point.forward_m, scale);
            (start <= forward && forward <= end).then_some(Vec3::new(
                point.lateral_m + shift,
                forward,
                height,
            ))
        })
        .collect()
}
