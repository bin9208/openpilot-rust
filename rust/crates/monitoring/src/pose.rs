use crate::scalar::{clamp, max, min};
use crate::{DriverMonitoring, Input, Policy, DT_DMON};

/// Original leveled driver camera geometry (tici/ar0231, 1928 x 1208).
pub fn face_orientation(
    orientation: [f64; 3],
    position: [f64; 2],
    calibration: [f64; 3],
) -> (f64, f64) {
    let pitch = orientation[0] + ((position[1] + 0.5) * 1208. - 604.).atan2(598.) - calibration[1];
    let yaw = -orientation[1] + ((position[0] + 0.5) * 1928. - 964.).atan2(598.) - calibration[2];
    (pitch, yaw)
}
impl DriverMonitoring {
    pub(crate) fn update_states(&mut self, input: &Input) {
        let driver = &input.driver;
        if input.car_speed > 11. && (driver.left.face_prob > 0.7 || driver.right.face_prob > 0.7) {
            self.wheelpos_offsetter.push(driver.wheel_on_right_prob);
        }
        self.wheel_on_right = if self.wheelpos_offsetter.filtered_stat.n >= 300 || input.demo {
            self.wheelpos_offsetter.filtered_stat.mean > 0.5
        } else {
            self.wheel_on_right_default
        };
        if input.enabled && !input.demo {
            if let Some(previous) = self.wheel_on_right_last {
                self.wheel_on_right = previous;
            }
        }
        let data = if self.wheel_on_right {
            &driver.right
        } else {
            &driver.left
        };
        let (Some(orientation), Some(position), Some(orientation_std), Some(_)) = (
            data.face_orientation,
            data.face_position,
            data.face_orientation_std,
            data.face_position_std,
        ) else {
            return;
        };
        self.face_detected = data.face_prob > 0.7;
        (self.pose.pitch, self.pose.yaw) =
            face_orientation(orientation, position, input.calibration);
        let steer = max(input.steering_angle_deg.abs() - 30., 0.);
        // np.sign(0) is zero; Rust signum(0) is one.
        let sign = if input.steering_angle_deg == 0. {
            0.
        } else {
            input.steering_angle_deg.signum()
        };
        self.pose.steer_yaw_offset = steer.to_radians() * -sign * 0.15;
        if self.wheel_on_right {
            self.pose.yaw *= -1.;
            self.pose.steer_yaw_offset *= -1.;
        }
        self.wheel_on_right_last = Some(self.wheel_on_right);
        self.model_std_max = max(orientation_std[0], orientation_std[1]);
        self.pose.low_std = self.model_std_max < 0.3;
        self.blink.left = data.left_blink_prob
            * f64::from(data.left_eye_prob > 0.65)
            * f64::from(data.sunglasses_prob < 0.9);
        self.blink.right = data.right_blink_prob
            * f64::from(data.right_eye_prob > 0.65)
            * f64::from(data.sunglasses_prob < 0.9);
        self.phone_prob = data.phone_prob;
        self.sleep_prob = data.sleep_prob;
        self.get_distracted_types();
        self.driver_distracted = (self.distracted_types.pose
            || self.distracted_types.eye
            || self.distracted_types.phone
            || self.distracted_types.sleep)
            && data.face_prob > 0.7
            && self.pose.low_std;
        let alpha = DT_DMON / (0.25 + DT_DMON);
        self.driver_distraction_filter = (1. - alpha) * self.driver_distraction_filter
            + alpha * f64::from(self.driver_distracted);
        if self.face_detected
            && input.car_speed > 13.
            && self.pose.low_std
            && (!input.enabled || !self.driver_distracted)
        {
            self.pose.pitch_offsetter.push(self.pose.pitch);
            self.pose.yaw_offsetter.push(self.pose.yaw);
        }
        self.pose.calibrated = self.pose.pitch_offsetter.filtered_stat.n >= 1200
            && self.pose.yaw_offsetter.filtered_stat.n >= 1200;
        if self.face_detected && !self.driver_distracted {
            let lowspeed = input.car_speed < 2.8;
            if self.model_std_max > 0.1 && !lowspeed {
                self.dcam_uncertain_cnt += 1;
                self.dcam_reset_cnt = 0;
            } else {
                self.dcam_reset_cnt += 1;
                if self.dcam_reset_cnt > 40 {
                    self.dcam_uncertain_cnt = 0;
                }
            }
        }
        self.is_model_uncertain = self.hi_stds >= 200;
        self.set_policy(if self.face_detected && !self.is_model_uncertain {
            Policy::Vision
        } else {
            Policy::Wheeltouch
        });
        if self.face_detected && !self.pose.low_std && !self.driver_distracted {
            self.hi_stds += 1;
        } else if self.face_detected && self.pose.low_std {
            self.hi_stds = 0;
        }
    }
    #[expect(
        clippy::approx_constant,
        reason = "0.3927 is the exact source policy threshold, not PI/8"
    )]
    fn get_distracted_types(&mut self) {
        let (mut pitch_error, mut yaw_error) = if self.pose.calibrated {
            (
                self.pose.pitch
                    - clamp(self.pose.pitch_offsetter.filtered_stat.mean, -0.0881, 0.124),
                self.pose.yaw - clamp(self.pose.yaw_offsetter.filtered_stat.mean, -0.0246, 0.289),
            )
        } else {
            (self.pose.pitch - 0.011, self.pose.yaw - 0.075)
        };
        pitch_error = if pitch_error > 0. {
            0.
        } else {
            pitch_error.abs()
        };
        yaw_error = if yaw_error * self.pose.steer_yaw_offset > 0. {
            max(
                yaw_error.abs() - min(self.pose.steer_yaw_offset.abs(), 0.3927),
                0.,
            )
        } else {
            yaw_error.abs()
        };
        let pitch_threshold = if self.pose.calibrated {
            0.3133 * self.pose.cfactor_pitch
        } else {
            0.449
        };
        self.distracted_types.pose =
            pitch_error > pitch_threshold || yaw_error > 0.4020 * self.pose.cfactor_yaw;
        self.distracted_types.eye = (self.blink.left + self.blink.right) * 0.5 > 0.865;
        self.distracted_types.phone = self.phone_prob > 0.5;
        self.distracted_types.sleep = self.sleep_prob > 0.75;
    }
}
