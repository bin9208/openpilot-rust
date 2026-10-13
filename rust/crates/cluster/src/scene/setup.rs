use super::{
    camera::{ego_anchor_x_m, scene_camera},
    display_state::display_state,
    geometry::LaneSpan,
    lines::{longitudinal_render_distance_scale, render_scene_forward_m},
    math::clamp,
    radar_candidates::radar_vehicle_points,
    road_style::data_geometry_mode_for_state,
    vehicles::radar_vehicle_box,
    CameraSpec, VehicleBox,
};
use crate::{
    config::{
        ClusterTheme, DEFAULT_LANE_WIDTH_M, EGO_FORWARD_M, ROAD_FAR_M, SURROUND_ROAD_FRONT_M,
        SURROUND_ROAD_REAR_M,
    },
    models::{ClusterUiState, RadarPoint},
};

pub(super) struct SceneSetup {
    pub width: f64,
    pub scale: f64,
    pub state: ClusterUiState,
    pub raw_corner_active: bool,
    pub anchor: f64,
    pub camera: CameraSpec,
    pub radar_points: Vec<RadarPoint>,
    pub radar_boxes: Vec<VehicleBox>,
    pub route_mode: bool,
    pub span: LaneSpan,
}

pub(super) fn setup(input: &ClusterUiState, theme: &ClusterTheme) -> SceneSetup {
    let width = clamp(
        if input.lane_width_m == 0.0 {
            DEFAULT_LANE_WIDTH_M
        } else {
            input.lane_width_m
        },
        2.4,
        4.6,
    );
    let scale = longitudinal_render_distance_scale(input);
    let (state, raw_corner_active) = display_state(input, width);
    let anchor = ego_anchor_x_m(&state, width);
    let camera = scene_camera(&state, width, anchor);
    let radar_points = radar_vehicle_points(&state, width);
    let radar_boxes: Vec<_> = radar_points
        .iter()
        .map(|point| radar_vehicle_box(point, &state, width, theme, 0.0))
        .collect();
    let route_mode = data_geometry_mode_for_state(&state);
    let drive_start = EGO_FORWARD_M - 5.0 - 8.0;
    let mut start = if state.surround_view_active {
        SURROUND_ROAD_REAR_M
    } else {
        drive_start
    };
    let end = if state.surround_view_active {
        SURROUND_ROAD_FRONT_M
    } else {
        ROAD_FAR_M
    };
    if (!state.detected_vehicles.is_empty() || !radar_boxes.is_empty())
        && !state.surround_view_active
    {
        let detected = state
            .detected_vehicles
            .iter()
            .map(|vehicle| render_scene_forward_m(vehicle.longitudinal_m, &state))
            .reduce(f64::min)
            .unwrap_or(ROAD_FAR_M);
        let radar = radar_boxes
            .iter()
            .map(|vehicle| vehicle.center.y)
            .reduce(f64::min)
            .unwrap_or(ROAD_FAR_M);
        start = start.min(drive_start.max((-35.0_f64).max(detected.min(radar) - 8.0)));
    }
    let span = LaneSpan {
        steering: state.steering,
        lane_width: width,
        start,
        end,
        steps: if state.surround_view_active {
            96
        } else if route_mode {
            48
        } else {
            64
        },
    };

    SceneSetup {
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
    }
}
