use crate::{
    live::projection::{animate, LaneSnapshot},
    models::ClusterUiState,
};

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct Metadata {
    pub road_curvature: Option<f64>,
    pub road_curvature_source: String,
    pub lane_position_source: String,
    pub model_path_source: String,
    pub lane_change_source: String,
    pub recenter_start: f64,
    pub continuation: bool,
    pub cutin_debug_text: String,
}
impl Default for Metadata {
    fn default() -> Self {
        Self {
            road_curvature: None,
            road_curvature_source: "steeringAngleDeg".into(),
            lane_position_source: "default".into(),
            model_path_source: "none".into(),
            lane_change_source: "none".into(),
            recenter_start: 1.0,
            continuation: false,
            cutin_debug_text: "NEW CUTIN: waiting".into(),
        }
    }
}
#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
pub struct Frame {
    pub t: f64,
    pub state: ClusterUiState,
    pub lanes: LaneSnapshot,
    pub metadata: Metadata,
}
impl Frame {
    #[must_use]
    pub fn to_state(&self) -> ClusterUiState {
        let mut state = self.state.clone();
        state.lane_width_m = state.lane_width_m.clamp(2.4, 4.6);
        let observed = self.lanes.center.map_or(0.0, |center| {
            (-center / state.lane_width_m).clamp(-1.25, 1.25)
        });
        state.highlight_lane.clone_from(&state.lane_change);
        let animation = animate(&mut state, self.metadata.recenter_start, observed);
        self.lanes.render(&mut state, &animation);
        state.ev_mode_active &= state.ev_mode_valid;
        state
    }
    #[must_use]
    pub fn blend(&self, right: &Self, amount: f64) -> Self {
        let mut result = if amount < 0.5 {
            self.clone()
        } else {
            right.clone()
        };
        let lerp = |left: f64, right: f64| left + (right - left) * amount;
        let optional = |left: Option<f64>, right: Option<f64>| match (left, right) {
            (Some(left), Some(right)) => Some(lerp(left, right)),
            (None, right) => right,
            (left, None) => left,
        };
        macro_rules! continuous { ($($field:ident),+ $(,)?) => { $(result.state.$field = lerp(self.state.$field, right.state.$field);)+ }; }
        macro_rules! optionals { ($($field:ident),+ $(,)?) => { $(result.state.$field = optional(self.state.$field, right.state.$field);)+ }; }
        result.t = lerp(self.t, right.t);
        continuous!(
            speed_kph,
            accel_mps2,
            steering,
            lane_width_m,
            throttle,
            brake,
            brake_disengage_risk,
            gas_disengage_risk,
            steer_override_risk,
            hard_brake_risk,
            gas_press_prob,
            brake_press_prob,
            disengage_risk,
            lane_change_prob,
            left_road_edge_confidence,
            right_road_edge_confidence
        );
        optionals!(
            display_speed_kph,
            steering_angle_deg,
            planned_speed_kph,
            planned_accel_mps2,
            planned_curvature_m_inv,
            model_turn_speed_kph,
            engaged_prob,
            left_lane_width_m,
            right_lane_width_m,
            left_road_edge_distance_m,
            right_road_edge_distance_m,
            frame_drop_perc,
            model_execution_time_ms,
            vision_speed_mps,
            vision_yaw_rate_rps,
            vision_speed_std_mps,
            vision_yaw_rate_std_rps,
            cpu_usage_percent,
            cpu_temp_c,
            memory_used_percent,
            disk_used_percent,
            longitudinal_t_follow_s,
            longitudinal_desired_distance_m,
            longitudinal_v_target_kph,
            longitudinal_jerk_target_mps3,
            lateral_plan_solver_cost
        );
        result.lanes.center = optional(self.lanes.center, right.lanes.center);
        for index in 0..2 {
            result.lanes.inner[index] = lerp(self.lanes.inner[index], right.lanes.inner[index]);
            result.lanes.edges[index] = optional(self.lanes.edges[index], right.lanes.edges[index]);
        }
        result.metadata.road_curvature =
            optional(self.metadata.road_curvature, right.metadata.road_curvature);
        result.state.ev_mode_active &= result.state.ev_mode_valid;
        result
    }
}
