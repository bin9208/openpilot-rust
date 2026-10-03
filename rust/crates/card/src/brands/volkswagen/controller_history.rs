use serde::{Deserialize, Serialize};
#[derive(Default, Serialize, Deserialize)]
pub struct History {
    pub frame: u64,
    pub apply_torque_last: i32,
    pub apply_curvature_last: f64,
    pub steering_power_last: i32,
    pub gra_acc_counter_last: Option<f64>,
    pub eps_timer_soft_disable_alert: bool,
    pub hca_frame_timer_running: u64,
    pub hca_frame_same_torque: u64,
    pub long_override_counter: u8,
    pub hold_release_frames: u32,
    pub long_disabled_counter: u8,
    pub klr_counter_last: Option<f64>,
    pub navi_event_last: i32,
    pub navi_banner_frames: u32,
    pub road_limit_last: f64,
    pub road_banner_frames: u32,
    pub lead_limit_disp: bool,
    pub lead_limit_cnt: u32,
    pub acc_hold_type_last: i32,
}
