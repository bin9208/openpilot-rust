use serde::{Deserialize, Serialize};

/// Model scalars promoted from cereal Float32 before policy arithmetic. Missing
/// arrays represent the original policy's empty-array early return.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DriverData {
    pub face_orientation: Option<Vec<f64>>,
    pub face_position: Option<Vec<f64>>,
    pub face_orientation_std: Option<Vec<f64>>,
    pub face_position_std: Option<Vec<f64>>,
    pub face_prob: f64,
    pub left_eye_prob: f64,
    pub right_eye_prob: f64,
    pub left_blink_prob: f64,
    pub right_blink_prob: f64,
    pub sunglasses_prob: f64,
    pub phone_prob: f64,
    pub sleep_prob: f64,
}
impl Default for DriverData {
    fn default() -> Self {
        Self {
            face_orientation: Some(vec![0.; 3]),
            face_position: Some(vec![0.; 2]),
            face_orientation_std: Some(vec![0.; 3]),
            face_position_std: Some(vec![0.; 2]),
            face_prob: 1.,
            left_eye_prob: 1.,
            right_eye_prob: 1.,
            left_blink_prob: 0.,
            right_blink_prob: 0.,
            sunglasses_prob: 0.,
            phone_prob: 0.,
            sleep_prob: 0.,
        }
    }
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct DriverState {
    pub left: DriverData,
    pub right: DriverData,
    pub wheel_on_right_prob: f64,
}
/// One driver-model cadence input. Gear membership is drive/low versus all
/// other original gear variants; the caller preserves that cereal distinction.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Input {
    pub driver: DriverState,
    pub car_speed: f64,
    pub enabled: bool,
    pub wrong_gear: bool,
    pub steering_pressed: bool,
    pub gas_pressed: bool,
    pub brake_disengage_prob: f64,
    pub steering_angle_deg: f64,
    pub calibration: Vec<f64>,
    pub demo: bool,
}
impl Default for Input {
    fn default() -> Self {
        Self {
            driver: DriverState::default(),
            car_speed: 20.,
            enabled: true,
            wrong_gear: false,
            steering_pressed: false,
            gas_pressed: false,
            brake_disengage_prob: 1.,
            steering_angle_deg: 0.,
            calibration: vec![0.; 3],
            demo: false,
        }
    }
}
