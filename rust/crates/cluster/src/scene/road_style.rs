use super::{
    math::{clamp, darken, lighten, rgba, rgba_with_alpha},
    Color,
};
use crate::{
    config::{ClusterTheme, AMBER, RED},
    models::{ClusterUiState, LaneMarking},
};
use num_traits::ToPrimitive;

pub type RoadEdgeLayer = (i64, Color, f64, f64);

#[must_use]
pub fn road_surface_offsets(state: &ClusterUiState, route_mode: bool) -> (f64, f64) {
    let shift = if route_mode {
        state.road_view_lane_position
    } else {
        0.0
    };
    let mut left = shift - 0.92;
    let mut right = shift + 0.92;
    if state.lane_change.as_deref() == Some("left") {
        left = left.min(shift - 1.55);
    } else if state.lane_change.as_deref() == Some("right") {
        right = right.max(shift + 1.55);
    }
    if state.extra_left_lane_visible && state.lane_change.as_deref() == Some("left") {
        left = (shift - 1.55).min(state.left_road_edge_offset.unwrap_or(shift - 1.9));
    } else if route_mode {
        if let Some(offset) = state.left_road_edge_offset {
            left = left.min(offset.max(-1.25));
        }
    }
    if state.extra_right_lane_visible && state.lane_change.as_deref() == Some("right") {
        right = (shift + 1.55).max(state.right_road_edge_offset.unwrap_or(shift + 1.9));
    } else if route_mode {
        if let Some(offset) = state.right_road_edge_offset {
            right = right.max(offset.min(1.25));
        }
    }
    (clamp(left, -2.8, -0.68), clamp(right, 0.68, 2.8))
}

#[must_use]
pub fn road_edge_color(distance: Option<f64>, confidence: f64, theme: &ClusterTheme) -> Color {
    let confidence = clamp(confidence, 0.0, 1.0);
    let (base, alpha) = if distance.is_some_and(|distance| distance < 0.85) {
        (RED, 165 + (80.0 * confidence).to_u8().unwrap_or(0))
    } else if distance.is_some_and(|distance| distance < 1.35) {
        (AMBER, 145 + (80.0 * confidence).to_u8().unwrap_or(0))
    } else {
        (
            theme.road_edge,
            150 + (70.0 * confidence).to_u8().unwrap_or(0),
        )
    };
    rgba(base, alpha.clamp(120, 245))
}

#[must_use]
pub fn road_edge_3d_layers(color: Color, theme: &ClusterTheme) -> [RoadEdgeLayer; 4] {
    let base = [color[0], color[1], color[2]];
    let alpha = f64::from(color[3]);
    let shadow = f64::from(theme.road_edge_backing[3]).max((alpha * 0.58).trunc());
    [
        (22, rgba_with_alpha(darken(base, 0.58), shadow), 0.032, 0.13),
        (
            14,
            rgba_with_alpha(darken(base, 0.30), alpha * 0.88),
            0.058,
            0.055,
        ),
        (8, color, 0.074, 0.0),
        (
            3,
            rgba_with_alpha(lighten(base, 0.38), alpha + 26.0),
            0.106,
            -0.045,
        ),
    ]
}

#[must_use]
pub fn lane_highlight_color(route_mode: bool) -> Color {
    [64, 148, 255, if route_mode { 170 } else { 220 }]
}

#[must_use]
pub fn bsd_lane_marking_offsets(state: &ClusterUiState) -> Vec<f64> {
    let mut offsets = Vec::new();
    if state.left_blindspot {
        offsets.push(
            if state.highlight_lane.as_deref() == Some("left") {
                state.highlight_lane_offset.unwrap_or(-1.0)
            } else {
                -1.0
            } + 0.5,
        );
    }
    if state.right_blindspot {
        offsets.push(
            if state.highlight_lane.as_deref() == Some("right") {
                state.highlight_lane_offset.unwrap_or(1.0)
            } else {
                1.0
            } - 0.5,
        );
    }
    offsets
}

#[must_use]
pub fn lane_marking_color_for_state(marking: &LaneMarking, offsets: &[f64]) -> [u8; 3] {
    if offsets
        .iter()
        .any(|offset| (marking.offset - offset).abs() <= 0.45)
    {
        RED
    } else {
        marking.color
    }
}

#[must_use]
pub fn data_geometry_mode_for_state(state: &ClusterUiState) -> bool {
    state.route_overlay.is_some()
        || !state.detected_vehicles.is_empty()
        || !state.radar_points.is_empty()
        || state.left_road_edge_offset.is_some()
        || state.right_road_edge_offset.is_some()
        || !state.left_road_edge_points.is_empty()
        || !state.right_road_edge_points.is_empty()
        || state
            .lanes
            .iter()
            .any(|marking| !marking.model_points.is_empty())
}
