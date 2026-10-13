#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SimulatorInput {
    pub throttle: f64,
    pub brake: f64,
    pub steering: f64,
    pub steering_angle_deg: Option<f64>,
    pub camera_lane_center_offset_m: Option<f64>,
    pub camera_lane_width_m: f64,
    pub surround_yaw_deg: f64,
    pub surround_pitch_deg: f64,
    pub surround_view_active: bool,
    pub left_signal_requested: bool,
    pub right_signal_requested: bool,
}
impl Default for SimulatorInput {
    fn default() -> Self {
        Self {
            throttle: 0.0,
            brake: 0.0,
            steering: 0.0,
            steering_angle_deg: None,
            camera_lane_center_offset_m: None,
            camera_lane_width_m: 3.6,
            surround_yaw_deg: 0.0,
            surround_pitch_deg: 0.0,
            surround_view_active: false,
            left_signal_requested: false,
            right_signal_requested: false,
        }
    }
}
