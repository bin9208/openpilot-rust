//! Final-source horizontal IMU impulse detector, independent of wheel-speed filtering.
use openpilot_calibrationd::orientation::rotation;
use openpilot_locationd::orientation::apply;
use serde::Serialize;
type Matrix = [[f64; 3]; 3];
#[derive(Default, Serialize)]
pub struct ImpactDetector {
    pub last_timestamp: f64,
    pub above: bool,
    orientation: Option<[f64; 3]>,
    calibration: Option<[f64; 3]>,
    gravity_device: Option<[f64; 3]>,
    device_from_calib: Option<Matrix>,
}
#[derive(Debug, PartialEq)]
pub struct Detection {
    pub trigger: bool,
    pub quiet: Option<bool>,
    pub acceleration: Option<[f64; 3]>,
}
impl ImpactDetector {
    pub fn update(
        &mut self,
        now: f64,
        timestamp: f64,
        sensor: &[f64],
        orientation: &[f64],
        calibration: &[f64],
        valid: bool,
    ) -> Detection {
        let invalid = Detection {
            trigger: false,
            quiet: Some(false),
            acceleration: None,
        };
        if !valid || !timestamp.is_finite() || !(0. ..=0.1).contains(&(now - timestamp)) {
            self.above = false;
            return invalid;
        }
        if timestamp <= self.last_timestamp {
            return Detection {
                trigger: false,
                quiet: None,
                acceleration: None,
            };
        }
        if [sensor, orientation, calibration]
            .iter()
            .any(|values| values.len() != 3 || !values.iter().all(|v| v.is_finite()))
        {
            self.above = false;
            return invalid;
        }
        let orientation: [f64; 3] = orientation.try_into().expect("validated vector");
        let calibration: [f64; 3] = calibration.try_into().expect("validated vector");
        let transpose =
            |matrix: Matrix| std::array::from_fn(|i| std::array::from_fn(|j| matrix[j][i]));
        if self.orientation != Some(orientation) {
            self.gravity_device = Some(apply(transpose(rotation(orientation)), [0., 0., -9.81]));
            self.orientation = Some(orientation);
        }
        if self.calibration != Some(calibration) {
            self.device_from_calib = Some(transpose(rotation(calibration)));
            self.calibration = Some(calibration);
        }
        let gravity = self
            .gravity_device
            .expect("orientation transform initialized");
        let device = [-sensor[2], -sensor[1], -sensor[0]];
        let acceleration = apply(
            self.device_from_calib
                .expect("calibration transform initialized"),
            std::array::from_fn(|i| device[i] - gravity[i]),
        );
        let horizontal = acceleration[0].hypot(acceleration[1]);
        let above = horizontal >= 1.5 * 9.81;
        let trigger = above && self.above && timestamp - self.last_timestamp <= 0.03;
        self.above = above;
        self.last_timestamp = timestamp;
        Detection {
            trigger,
            quiet: Some(horizontal < 10.),
            acceleration: Some(acceleration),
        }
    }
}
