use super::{clamp, smoothstep, ClusterSimulator, Direction, Phase};
use crate::models::SimulatorInput;
impl ClusterSimulator {
    pub(super) fn start_signal(&mut self, direction: Direction) {
        match direction {
            Direction::Left => {
                self.left_signal_until = self.elapsed + 5.4;
                self.right_signal_until = self.right_signal_until.min(self.elapsed);
            }
            Direction::Right => {
                self.right_signal_until = self.elapsed + 5.4;
                self.left_signal_until = self.left_signal_until.min(self.elapsed);
            }
        }
        self.lane_change_direction = Some(direction);
        self.lane_change_phase = Phase::Changing;
        self.lane_change_elapsed = 0.0;
        self.lane_change_progress = 0.0;
        self.lane_change_recenter_start_progress = 1.0;
        self.target_lane_position = self.active_lane_position + direction.sign();
    }
    pub(super) fn update_lane_change(&mut self, dt: f64) {
        match self.lane_change_phase {
            Phase::Idle => {
                self.lane_change_progress = 0.0;
                self.ego_lane_position +=
                    (self.active_lane_position - self.ego_lane_position) * (dt / 0.8).min(1.0);
                self.view_lane_position +=
                    (self.active_lane_position - self.view_lane_position) * (dt / 0.8).min(1.0);
            }
            Phase::Changing => {
                self.lane_change_elapsed += dt;
                let duration = clamp(4.8 - self.speed_kph * 0.032, 2.2, 4.8);
                self.lane_change_progress = clamp(self.lane_change_elapsed / duration, 0.0, 1.0);
                let direction = self.lane_change_direction.map_or(1.0, Direction::sign);
                self.ego_lane_position =
                    self.active_lane_position + direction * self.lane_change_progress;
                if self.lane_change_progress >= 1.0 {
                    self.active_lane_position = self.target_lane_position;
                    self.ego_lane_position = self.target_lane_position;
                    self.lane_change_phase = Phase::Recentering;
                    self.lane_change_elapsed = 0.0;
                    self.lane_change_recenter_start_progress = 1.0;
                }
            }
            Phase::Recentering => {
                self.lane_change_elapsed += dt;
                self.lane_change_progress = clamp(self.lane_change_elapsed / 0.85, 0.0, 1.0);
                let direction = self.lane_change_direction.map_or(1.0, Direction::sign);
                let blend = smoothstep(self.lane_change_progress);
                let start = direction * smoothstep(self.lane_change_recenter_start_progress);
                self.view_lane_position = self.active_lane_position - start * (1.0 - blend);
                self.ego_lane_position = self.active_lane_position;
                if self.lane_change_progress >= 1.0 {
                    self.view_lane_position = self.active_lane_position;
                    self.lane_change_phase = Phase::Idle;
                    self.lane_change_progress = 0.0;
                    self.lane_change_recenter_start_progress = 1.0;
                    self.lane_change_direction = None;
                }
            }
        }
    }
    pub(super) fn apply_camera_lane_model(&mut self, command: &SimulatorInput) {
        self.lane_width_m = clamp(command.camera_lane_width_m, 2.4, 4.6);
        self.camera_lane_center_offset_m = command.camera_lane_center_offset_m;
        let Some(center) = command.camera_lane_center_offset_m else {
            return;
        };
        let offset = clamp(center / self.lane_width_m, -1.4, 1.4);
        let mut observed = self.active_lane_position - offset;
        if self.lane_change_direction.is_some() {
            let low = self.active_lane_position.min(self.target_lane_position) - 0.25;
            let high = self.active_lane_position.max(self.target_lane_position) + 0.25;
            observed = clamp(observed, low, high);
        }
        self.ego_lane_position = self.ego_lane_position * 0.45 + observed * 0.55;
        if self.lane_change_phase == Phase::Changing
            && self
                .target_lane_position
                .partial_cmp(&self.active_lane_position)
                != Some(std::cmp::Ordering::Equal)
        {
            let measured = ((self.ego_lane_position - self.active_lane_position)
                / (self.target_lane_position - self.active_lane_position))
                .abs();
            let progress = clamp(measured, 0.0, 1.0);
            if progress > self.lane_change_progress {
                self.lane_change_progress = progress;
            }
            if self.lane_change_progress >= 0.98 {
                self.active_lane_position = self.target_lane_position;
                self.ego_lane_position = self.target_lane_position;
                self.lane_change_phase = Phase::Recentering;
                self.lane_change_elapsed = 0.0;
                self.lane_change_recenter_start_progress =
                    clamp(self.lane_change_progress, 0.0, 1.0);
            }
        }
    }
}
