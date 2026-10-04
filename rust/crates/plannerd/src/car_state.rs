use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct CarState {
    pub v_ego: f64,
    pub a_ego: f64,
    pub v_ego_cluster: f64,
    pub v_clu_ratio: f64,
    pub gas_pressed: bool,
    pub brake_pressed: bool,
    pub standstill: bool,
    pub steering_angle_deg: f64,
    pub steering_torque: f64,
    pub steering_pressed: bool,
    pub left_blinker: bool,
    pub right_blinker: bool,
    pub left_blindspot: bool,
    pub right_blindspot: bool,
    pub soft_hold_active: i32,
    pub carrot_cruise: i32,
    pub use_lane_line_speed: f64,
    pub v_cruise: f64,
    pub v_cruise_cluster: f64,
}
