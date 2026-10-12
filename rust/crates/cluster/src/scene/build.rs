use super::{
    build_vehicles::{build_scene_vehicles, marker_hidden_points, VehicleSceneInput},
    geometry::LaneSpan,
    geometry_cache::GeometryCache,
    lines::{lane_floor_strip, ModelSpan},
    math::rgba,
    path::planned_path_strips,
    profile::{record, ModelProfile, ProfileAdd},
    road_edges::{road_edge_strips, EdgeCaches, RoadEdgeCache},
    road_style::{bsd_lane_marking_offsets, lane_highlight_color, lane_marking_color_for_state},
    setup::{setup, SceneSetup},
    style_cache::StyleCache,
    vehicles::{radar_point_markers, MarkerViewport},
    ClusterScene, MeshStrip,
};
use crate::{
    config::{ClusterTheme, ROAD_FAR_M},
    models::ClusterUiState,
};
use std::time::Instant;

#[derive(Default)]
pub struct SceneBuilder {
    geometry: GeometryCache,
    styles: StyleCache,
    edges: RoadEdgeCache,
}

impl SceneBuilder {
    #[must_use]
    pub fn build(
        &mut self,
        input: &ClusterUiState,
        highlight_lit: bool,
        theme: &ClusterTheme,
        mut profile: ProfileAdd<'_>,
    ) -> ClusterScene {
        let started = Instant::now();
        let SceneSetup {
            width,
            scale,
            state,
            raw_corner_active,
            anchor,
            camera,
            radar_points,
            radar_boxes,
            route_mode,
            span,
        } = setup(input, theme);
        record(&mut profile, "scene.build.setup", started);
        let started = Instant::now();
        let highlight_lanes = state
            .highlight_lane_offset
            .filter(|_| highlight_lit)
            .and_then(|offset| {
                lane_floor_strip(
                    &state,
                    offset,
                    lane_highlight_color(route_mode),
                    &span,
                    route_mode,
                    0.006,
                )
            })
            .into_iter()
            .collect();
        record(&mut profile, "scene.build.highlight_lanes", started);
        let started = Instant::now();
        let lane_markings = self.lane_markings(&state, &span, scale, theme, &mut profile);
        record(&mut profile, "scene.build.lane_markings", started);
        let started = Instant::now();
        let vehicles = build_scene_vehicles(&VehicleSceneInput {
            state: &state,
            width,
            relative_x: anchor,
            route_mode,
            raw_corner_active,
            radar_points: &radar_points,
            radar_boxes: &radar_boxes,
            theme,
        });
        record(&mut profile, "scene.build.vehicles", started);
        let started = Instant::now();
        let road_surface = MeshStrip::new(Vec::new(), Vec::new(), rgba(theme.road, 255));
        record(&mut profile, "scene.build.road_surface", started);
        let started = Instant::now();
        let road_edges = self.build_road_edges(&state, route_mode, &span, theme, &mut profile);
        record(&mut profile, "scene.build.road_edges", started);
        let started = Instant::now();
        let planned_path = planned_path_strips(
            &mut self.geometry,
            &state,
            width,
            &vehicles.blockers,
            theme,
            &mut profile,
        );
        record(&mut profile, "scene.build.planned_path", started);
        let started = Instant::now();
        let hidden = marker_hidden_points(
            radar_points,
            &state,
            raw_corner_active,
            &vehicles.merged_radar_labels,
        );
        let radar_points = radar_point_markers(
            &state,
            width,
            &hidden,
            &MarkerViewport {
                min_forward: span.start,
                max_forward: if state.surround_view_active {
                    span.end
                } else {
                    ROAD_FAR_M + 30.0
                },
                x_offset: anchor,
                lateral_speed_offset: 0.0,
            },
        );
        record(&mut profile, "scene.build.radar_points", started);
        let started = Instant::now();
        let scene = ClusterScene {
            camera,
            scene_shift_x_m: -anchor,
            road_surface,
            road_edges,
            highlight_lanes,
            lane_markings,
            planned_path,
            radar_points,
            vehicles: vehicles.vehicles,
        };
        record(&mut profile, "scene.build.pack", started);
        scene
    }

    fn build_road_edges(
        &mut self,
        state: &ClusterUiState,
        route_mode: bool,
        span: &LaneSpan,
        theme: &ClusterTheme,
        profile: &mut ProfileAdd<'_>,
    ) -> Vec<MeshStrip> {
        let geometry_started = Instant::now();
        let raw_edges = road_edge_strips(
            &mut EdgeCaches {
                edges: &mut self.edges,
                geometry: &mut self.geometry,
                styles: &mut self.styles,
            },
            state,
            route_mode,
            span,
            theme,
            profile,
        );
        record(profile, "scene.build.road_edges.geometry", geometry_started);
        let merge_started = Instant::now();
        let road_edges = self.styles.merge_by_style(raw_edges);
        record(profile, "scene.build.road_edges.merge", merge_started);
        road_edges
    }

    fn lane_markings(
        &mut self,
        state: &ClusterUiState,
        span: &LaneSpan,
        scale: f64,
        theme: &ClusterTheme,
        profile: &mut ProfileAdd<'_>,
    ) -> Vec<MeshStrip> {
        let offsets = bsd_lane_marking_offsets(state);
        let mut strips = Vec::new();
        let mut model_ms = 0.0;
        let mut offset_ms = 0.0;
        let mut collect_ms = 0.0;
        for marking in state.lanes.iter().filter(|marking| marking.visible) {
            let specs = [
                (marking.width + 3, theme.lane_marking_border, 0.026),
                (
                    marking.width,
                    rgba(lane_marking_color_for_state(marking, &offsets), 255),
                    0.044,
                ),
            ];
            let mut groups = None;
            if !marking.model_points.is_empty() {
                let started = Instant::now();
                let model_span = ModelSpan {
                    start: span.start,
                    end: span.end,
                    steps: span.steps,
                    height: 0.0,
                    scale,
                    extend_before_model: true,
                };
                groups = self
                    .geometry
                    .model_line_groups(
                        &marking.model_points,
                        &model_span,
                        &specs,
                        &marking.style,
                        &mut ModelProfile {
                            callback: profile,
                            prefix: "scene.lane_model",
                        },
                    )
                    .map(|groups| {
                        let started = Instant::now();
                        let groups =
                            self.styles
                                .style(&groups, &specs, marking.model_lateral_shift_m);
                        record(profile, "scene.lane_model.style", started);
                        groups
                    });
                model_ms += started.elapsed().as_secs_f64() * 1000.0;
            }
            let groups = groups.unwrap_or_else(|| {
                let started = Instant::now();
                let groups =
                    self.geometry
                        .lane_offset_groups(marking.offset, span, &specs, &marking.style);
                let groups = self.styles.style(&groups, &specs, 0.0);
                offset_ms += started.elapsed().as_secs_f64() * 1000.0;
                groups
            });
            let started = Instant::now();
            strips.extend(groups.iter().flatten().cloned());
            collect_ms += started.elapsed().as_secs_f64() * 1000.0;
        }
        if let Some(add) = profile {
            add("scene.build.lane_markings.model", model_ms);
            add("scene.build.lane_markings.offset", offset_ms);
            add("scene.build.lane_markings.collect", collect_ms);
        }
        let started = Instant::now();
        let strips = self.styles.merge_by_style(strips);
        record(profile, "scene.build.lane_markings.merge", started);
        strips
    }
}
