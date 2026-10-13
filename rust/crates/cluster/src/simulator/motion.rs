use super::{clamp, ClusterSimulator};
use crate::models::SimulatorInput;
impl ClusterSimulator {
    pub(super) fn update_steering(&mut self, command: &SimulatorInput) {
        let target = if let Some(angle) = command.steering_angle_deg {
            let angle = clamp(angle, -45.0, 45.0);
            self.steering_angle_deg = Some(angle);
            angle / 45.0
        } else {
            let target = clamp(command.steering, -1.0, 1.0);
            self.steering_angle_deg = Some(target * 45.0);
            target
        };
        self.steering = self.steering * 0.72 + target * 0.28;
    }
    pub(super) fn update_motion(&mut self, command: &SimulatorInput, dt: f64) {
        let throttle = clamp(command.throttle, 0.0, 1.0);
        let brake = clamp(command.brake, 0.0, 1.0);
        let speed = self.speed_kph / 3.6;
        let mut accel = throttle * 3.2 - brake * 5.0 - speed * 0.012;
        if throttle == 0.0 && brake == 0.0 && speed > 0.0 {
            accel -= 0.18;
        }
        let mut next = self.speed_kph + accel * dt * 3.6;
        if next <= 0.0 {
            next = 0.0;
            accel = if accel > 0.0 { accel } else { 0.0 };
        }
        self.speed_kph = clamp(next, 0.0, 260.0);
        self.accel_mps2 = clamp(accel, -5.0, 5.0);
    }
    pub(super) fn update_surround_view(&mut self, command: &SimulatorInput, dt: f64) {
        let target_yaw = if command.surround_view_active {
            clamp(command.surround_yaw_deg, -180.0, 180.0)
        } else {
            0.0
        };
        let target_pitch = if command.surround_view_active {
            clamp(command.surround_pitch_deg, -18.0, 18.0)
        } else {
            0.0
        };
        let alpha = (dt / 0.16).min(1.0);
        self.surround_yaw_deg += (target_yaw - self.surround_yaw_deg) * alpha;
        self.surround_pitch_deg += (target_pitch - self.surround_pitch_deg) * alpha;
        self.surround_view_active = command.surround_view_active
            || self.surround_yaw_deg.abs() > 0.4
            || self.surround_pitch_deg.abs() > 0.4;
    }
}
