use super::profile::{record, ModelProfile, ProfileAdd};
use super::{
    cache::{cache_grid_value, model_line_cache_end_m, model_line_cache_start_m, Cache},
    geometry::{lane_centerline, LaneSpan},
    geometry_cache::GeometryCache,
    lines::{longitudinal_render_distance_scale, ModelSpan},
    math::clamp,
    road_style::{road_edge_3d_layers, road_edge_color, road_surface_offsets, RoadEdgeLayer},
    strips::{lane_marking_strip_groups_from_segments, MarkingSpec, MeshStripGroups},
    style_cache::StyleCache,
    Color, MeshStrip,
};
use crate::{
    config::ClusterTheme,
    models::{ClusterUiState, ModelPathPoint},
};
use std::time::Instant;

type EdgeKey = (f64, f64, f64, f64, f64, f64, Vec<(i64, f64, f64)>);

pub struct RoadEdgeCache {
    offset: Cache<EdgeKey, MeshStripGroups>,
}
impl Default for RoadEdgeCache {
    fn default() -> Self {
        Self {
            offset: Cache::new(48),
        }
    }
}

pub struct EdgeStyle<'a> {
    pub color: Color,
    pub side: f64,
    pub theme: &'a ClusterTheme,
}

fn layer_specs(layers: &[RoadEdgeLayer]) -> Vec<MarkingSpec> {
    layers
        .iter()
        .map(|(width, color, height, _)| (*width, *color, *height))
        .collect()
}

impl RoadEdgeCache {
    fn offset_groups(
        &mut self,
        offset: f64,
        side: f64,
        span: &LaneSpan,
        layers: &[RoadEdgeLayer],
    ) -> MeshStripGroups {
        let offset = cache_grid_value(offset, 0.01);
        let steering = cache_grid_value(span.steering, 0.002);
        let width = 0.1_f64.max(cache_grid_value(span.lane_width, 0.02));
        let side = if side < 0.0 { -1.0 } else { 1.0 };
        let start = model_line_cache_start_m(span.start);
        let end = model_line_cache_end_m(span.end);
        let geometry: Vec<_> = layers
            .iter()
            .map(|(width, _, height, shift)| (*width, *height, *shift))
            .collect();
        let key = (offset, steering, width, side, start, end, geometry.clone());
        if let Some(groups) = self.offset.get(&key) {
            return groups;
        }
        let groups: MeshStripGroups = geometry
            .iter()
            .map(|(pixels, height, shift)| {
                let centerline = lane_centerline(
                    offset + side * shift / width,
                    steering,
                    width,
                    start,
                    end,
                    56,
                    0.0,
                );
                lane_marking_strip_groups_from_segments(
                    &[centerline],
                    &[(*pixels, [0; 4], *height)],
                )[0]
                .clone()
            })
            .collect::<Vec<_>>()
            .into();
        self.offset.insert(key, groups.clone());
        groups
    }

    #[must_use]
    pub fn offset_strips(
        &mut self,
        styles: &mut StyleCache,
        offset: f64,
        span: &LaneSpan,
        style: &EdgeStyle<'_>,
    ) -> Vec<MeshStrip> {
        let layers = road_edge_3d_layers(style.color, style.theme);
        let groups = self.offset_groups(offset, style.side, span, &layers);
        let specs = layer_specs(&layers);
        styles
            .style_by_shift(&groups, &specs, &vec![0.0; specs.len()])
            .iter()
            .flatten()
            .cloned()
            .collect()
    }
}

#[must_use]
pub fn road_edge_model_strips(
    geometry: &mut GeometryCache,
    styles: &mut StyleCache,
    points: &[ModelPathPoint],
    shift: f64,
    span: &ModelSpan,
    style: &EdgeStyle<'_>,
    profile: &mut ProfileAdd<'_>,
) -> Vec<MeshStrip> {
    let layers = road_edge_3d_layers(style.color, style.theme);
    let specs = layer_specs(&layers);
    let Some(groups) = geometry.model_line_groups(
        points,
        span,
        &specs,
        "solid",
        &mut ModelProfile {
            callback: profile,
            prefix: "scene.road_model",
        },
    ) else {
        return Vec::new();
    };
    let shifts: Vec<_> = layers
        .iter()
        .map(|(_, _, _, offset)| shift + style.side * offset)
        .collect();
    let started = Instant::now();
    let strips = styles
        .style_by_shift(&groups, &specs, &shifts)
        .iter()
        .flatten()
        .cloned()
        .collect();
    record(profile, "scene.road_model.style", started);
    strips
}

pub struct EdgeCaches<'a> {
    pub edges: &'a mut RoadEdgeCache,
    pub geometry: &'a mut GeometryCache,
    pub styles: &'a mut StyleCache,
}

#[must_use]
pub fn road_edge_strips(
    caches: &mut EdgeCaches<'_>,
    state: &ClusterUiState,
    route_mode: bool,
    span: &LaneSpan,
    theme: &ClusterTheme,
    profile: &mut ProfileAdd<'_>,
) -> Vec<MeshStrip> {
    if !route_mode {
        let (left, right) = road_surface_offsets(state, route_mode);
        let color = road_edge_color(None, 1.0, theme);
        let mut strips = caches.edges.offset_strips(
            caches.styles,
            left,
            span,
            &EdgeStyle {
                color,
                side: -1.0,
                theme,
            },
        );
        strips.extend(caches.edges.offset_strips(
            caches.styles,
            right,
            span,
            &EdgeStyle {
                color,
                side: 1.0,
                theme,
            },
        ));
        return strips;
    }
    let mut strips = Vec::new();
    for (points, shift, offset, distance, confidence, side) in [
        (
            &state.left_road_edge_points,
            state.left_road_edge_lateral_shift_m,
            state.left_road_edge_offset,
            state.left_road_edge_distance_m,
            state.left_road_edge_confidence,
            -1.0,
        ),
        (
            &state.right_road_edge_points,
            state.right_road_edge_lateral_shift_m,
            state.right_road_edge_offset,
            state.right_road_edge_distance_m,
            state.right_road_edge_confidence,
            1.0,
        ),
    ] {
        let style = EdgeStyle {
            color: road_edge_color(distance, confidence, theme),
            side,
            theme,
        };
        if !points.is_empty() {
            let model_span = ModelSpan {
                start: span.start,
                end: span.end,
                steps: span.steps,
                height: 0.0,
                scale: longitudinal_render_distance_scale(state),
                extend_before_model: true,
            };
            strips.extend(road_edge_model_strips(
                caches.geometry,
                caches.styles,
                points,
                shift,
                &model_span,
                &style,
                profile,
            ));
        } else if let Some(offset) = offset {
            let offset = if side < 0.0 {
                clamp(offset, -2.8, -0.68)
            } else {
                clamp(offset, 0.68, 2.8)
            };
            strips.extend(
                caches
                    .edges
                    .offset_strips(caches.styles, offset, span, &style),
            );
        }
    }
    strips
}
