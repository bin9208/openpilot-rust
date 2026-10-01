use crate::{
    points::Point,
    pose::{Calibrator, Pose},
    settings::Settings,
};
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Input {
    Control { active: bool },
    State { speed: f64, pressed: bool },
    Controls { saturated: bool, curvature: f64 },
    Calibration { rpy: [f64; 3], valid: bool },
    Pose { pose: Pose, valid: bool },
}
#[derive(Default, Debug, serde::Serialize)]
pub struct Motion {
    pub t: f64,
    pub lat_active: bool,
    pub steering_pressed: bool,
    pub steering_saturated: bool,
    pub desired_curvature: f64,
    pub v_ego: f64,
    pub yaw_rate: f64,
    pub yaw_rate_std: f64,
    pub pose_valid: bool,
    pub recovery_times: [f64; 4],
    pub calibrator: Calibrator,
}
impl Motion {
    pub fn handle(&mut self, time: f64, input: Input) {
        match input {
            Input::Control { active } => self.lat_active = active,
            Input::State { speed, pressed } => {
                self.v_ego = speed;
                self.steering_pressed = pressed;
            }
            Input::Controls {
                saturated,
                curvature,
            } => {
                self.steering_saturated = saturated;
                self.desired_curvature = curvature;
            }
            Input::Calibration { rpy, valid } => self.calibrator.feed(rpy, valid),
            Input::Pose { pose, valid } => {
                let calibrated = self.calibrator.pose(pose);
                self.yaw_rate = calibrated.angular_velocity.xyz[2];
                self.yaw_rate_std = calibrated.angular_velocity.std[2];
                self.pose_valid = valid;
            }
        }
        self.t = time;
    }
    pub fn point(&mut self, settings: &Settings) -> Point {
        let desired = self.desired_curvature * self.v_ego * self.v_ego;
        let actual = self.yaw_rate * self.v_ego;
        let fast = self.v_ego > settings.min_vego;
        let turning = self.yaw_rate.abs() >= settings.min_yr;
        let sensors = self.pose_valid && self.yaw_rate.abs() < 1. && self.yaw_rate_std < 1.;
        let lateral = actual.abs() <= settings.max_lat_accel
            && (desired - actual).abs() <= settings.max_lat_accel_diff;
        for (last, violated) in self.recovery_times.iter_mut().zip([
            !self.lat_active,
            self.steering_pressed,
            self.steering_saturated,
            !sensors || !lateral,
        ]) {
            if violated {
                *last = self.t;
            }
        }
        let recovered = self
            .recovery_times
            .iter()
            .all(|last| self.t - last >= settings.min_recovery_buffer_sec);
        let okay = self.lat_active
            && !self.steering_pressed
            && !self.steering_saturated
            && fast
            && turning
            && recovered
            && self.calibrator.valid
            && sensors
            && lateral;
        Point {
            time: self.t,
            desired,
            actual,
            okay,
        }
    }
}
