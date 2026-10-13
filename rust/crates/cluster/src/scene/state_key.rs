use crate::models::{
    ClusterUiState, CruiseDisplayState, DetectedVehicle, LaneMarking, ModelPathPoint, RadarPoint,
    RouteOverlay,
};

#[derive(Clone, Debug, PartialEq)]
pub struct SceneStateKey {
    visibility: [bool; 5],
    speed_kph: f64,
    steering: f64,
    vision_yaw_rate_rps: Option<f64>,
    camera_view_mode: i64,
    surround_yaw_deg: f64,
    surround_pitch_deg: f64,
    lane_width_m: f64,
    lanes: Vec<LaneMarking>,
    lane_change: Option<String>,
    lane_change_phase: String,
    highlight_lane: Option<String>,
    highlight_lane_offset: Option<f64>,
    ego_lane_offset: f64,
    road_view_lane_position: f64,
    model_path: Vec<ModelPathPoint>,
    route_overlay: Option<RouteOverlay>,
    detected_vehicles: Vec<DetectedVehicle>,
    radar_points: Vec<RadarPoint>,
    radar_display_mode: i64,
    radar_source_color_mode: i64,
    left_road_edge_offset: Option<f64>,
    right_road_edge_offset: Option<f64>,
    left_road_edge_points: Vec<ModelPathPoint>,
    right_road_edge_points: Vec<ModelPathPoint>,
    left_road_edge_lateral_shift_m: f64,
    right_road_edge_lateral_shift_m: f64,
    left_road_edge_distance_m: Option<f64>,
    right_road_edge_distance_m: Option<f64>,
    left_road_edge_confidence: f64,
    right_road_edge_confidence: f64,
    cruise_display_state: CruiseDisplayState,
    cruise_gap: Option<i64>,
    longitudinal_t_follow_s: Option<f64>,
}
impl SceneStateKey {
    #[must_use]
    pub fn from_state(state: &ClusterUiState) -> Self {
        Self {
            visibility: [
                state.surround_view_active,
                state.extra_left_lane_visible,
                state.extra_right_lane_visible,
                state.left_blindspot,
                state.right_blindspot,
            ],
            speed_kph: state.speed_kph,
            steering: state.steering,
            vision_yaw_rate_rps: state.vision_yaw_rate_rps,
            camera_view_mode: state.camera_view_mode,
            surround_yaw_deg: state.surround_yaw_deg,
            surround_pitch_deg: state.surround_pitch_deg,
            lane_width_m: state.lane_width_m,
            lanes: state.lanes.clone(),
            lane_change: state.lane_change.clone(),
            lane_change_phase: state.lane_change_phase.clone(),
            highlight_lane: state.highlight_lane.clone(),
            highlight_lane_offset: state.highlight_lane_offset,
            ego_lane_offset: state.ego_lane_offset,
            road_view_lane_position: state.road_view_lane_position,
            model_path: state.model_path.clone(),
            route_overlay: state.route_overlay.clone(),
            detected_vehicles: state.detected_vehicles.clone(),
            radar_points: state.radar_points.clone(),
            radar_display_mode: state.radar_display_mode,
            radar_source_color_mode: state.radar_source_color_mode,
            left_road_edge_offset: state.left_road_edge_offset,
            right_road_edge_offset: state.right_road_edge_offset,
            left_road_edge_points: state.left_road_edge_points.clone(),
            right_road_edge_points: state.right_road_edge_points.clone(),
            left_road_edge_lateral_shift_m: state.left_road_edge_lateral_shift_m,
            right_road_edge_lateral_shift_m: state.right_road_edge_lateral_shift_m,
            left_road_edge_distance_m: state.left_road_edge_distance_m,
            right_road_edge_distance_m: state.right_road_edge_distance_m,
            left_road_edge_confidence: state.left_road_edge_confidence,
            right_road_edge_confidence: state.right_road_edge_confidence,
            cruise_display_state: state.cruise_display_state,
            cruise_gap: state.cruise_gap,
            longitudinal_t_follow_s: state.longitudinal_t_follow_s,
        }
    }
    #[must_use]
    pub fn matches(&self, state: &ClusterUiState) -> bool {
        self.visibility
            == [
                state.surround_view_active,
                state.extra_left_lane_visible,
                state.extra_right_lane_visible,
                state.left_blindspot,
                state.right_blindspot,
            ]
            && self.speed_kph.eq(&state.speed_kph)
            && self.steering.eq(&state.steering)
            && self.vision_yaw_rate_rps.eq(&state.vision_yaw_rate_rps)
            && self.camera_view_mode.eq(&state.camera_view_mode)
            && self.surround_yaw_deg.eq(&state.surround_yaw_deg)
            && self.surround_pitch_deg.eq(&state.surround_pitch_deg)
            && self.lane_width_m.eq(&state.lane_width_m)
            && self.lanes.eq(&state.lanes)
            && self.lane_change.eq(&state.lane_change)
            && self.lane_change_phase.eq(&state.lane_change_phase)
            && self.highlight_lane.eq(&state.highlight_lane)
            && self.highlight_lane_offset.eq(&state.highlight_lane_offset)
            && self.ego_lane_offset.eq(&state.ego_lane_offset)
            && self
                .road_view_lane_position
                .eq(&state.road_view_lane_position)
            && self.model_path.eq(&state.model_path)
            && self.route_overlay.eq(&state.route_overlay)
            && self.detected_vehicles.eq(&state.detected_vehicles)
            && self.radar_points.eq(&state.radar_points)
            && self.radar_display_mode.eq(&state.radar_display_mode)
            && self
                .radar_source_color_mode
                .eq(&state.radar_source_color_mode)
            && self.left_road_edge_offset.eq(&state.left_road_edge_offset)
            && self
                .right_road_edge_offset
                .eq(&state.right_road_edge_offset)
            && self.left_road_edge_points.eq(&state.left_road_edge_points)
            && self
                .right_road_edge_points
                .eq(&state.right_road_edge_points)
            && self
                .left_road_edge_lateral_shift_m
                .eq(&state.left_road_edge_lateral_shift_m)
            && self
                .right_road_edge_lateral_shift_m
                .eq(&state.right_road_edge_lateral_shift_m)
            && self
                .left_road_edge_distance_m
                .eq(&state.left_road_edge_distance_m)
            && self
                .right_road_edge_distance_m
                .eq(&state.right_road_edge_distance_m)
            && self
                .left_road_edge_confidence
                .eq(&state.left_road_edge_confidence)
            && self
                .right_road_edge_confidence
                .eq(&state.right_road_edge_confidence)
            && self.cruise_display_state.eq(&state.cruise_display_state)
            && self.cruise_gap.eq(&state.cruise_gap)
            && self
                .longitudinal_t_follow_s
                .eq(&state.longitudinal_t_follow_s)
    }
}

pub const SCENE_STATE_FIELDS: [&str; 38] = [
    "speed_kph",
    "steering",
    "vision_yaw_rate_rps",
    "camera_view_mode",
    "surround_view_active",
    "surround_yaw_deg",
    "surround_pitch_deg",
    "lane_width_m",
    "lanes",
    "lane_change",
    "lane_change_phase",
    "highlight_lane",
    "highlight_lane_offset",
    "ego_lane_offset",
    "road_view_lane_position",
    "extra_left_lane_visible",
    "extra_right_lane_visible",
    "left_blindspot",
    "right_blindspot",
    "model_path",
    "route_overlay",
    "detected_vehicles",
    "radar_points",
    "radar_display_mode",
    "radar_source_color_mode",
    "left_road_edge_offset",
    "right_road_edge_offset",
    "left_road_edge_points",
    "right_road_edge_points",
    "left_road_edge_lateral_shift_m",
    "right_road_edge_lateral_shift_m",
    "left_road_edge_distance_m",
    "right_road_edge_distance_m",
    "left_road_edge_confidence",
    "right_road_edge_confidence",
    "cruise_display_state",
    "cruise_gap",
    "longitudinal_t_follow_s",
];
