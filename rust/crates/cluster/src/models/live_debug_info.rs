#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub struct LiveDebugInfo {
    pub live_delay_calibration_percent: Option<f64>,
    pub live_delay_lateral_s: Option<f64>,
    pub live_torque_calibration_percent: Option<f64>,
    pub live_torque_valid: Option<bool>,
    pub live_torque_lat_accel_factor: Option<f64>,
    pub live_torque_friction: Option<f64>,
    pub live_steer_ratio: Option<f64>,
    pub custom_steer_ratio: Option<f64>,
    pub steer_actuator_delay_s: Option<f64>,
}
