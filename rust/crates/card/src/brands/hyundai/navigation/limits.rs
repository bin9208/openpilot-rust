use super::{Input, Navigation, Output};

impl Navigation {
    pub fn speed_limit(
        &mut self,
        output: &mut Output,
        input: &Input<'_>,
        mut camera: bool,
        speed: f64,
        distance_time_changed: bool,
    ) {
        if self.controlled_access(input) && output.speed_limit == 30. {
            camera = false;
        }
        self.total_distance += speed * 0.01;
        if output.speed_limit > 0. && camera && self.mode != 0 && self.camera_target.is_some() {
            if let Some(target) = self.camera_target {
                self.speed_limit_distance = target;
                output.speed_limit_distance =
                    (target - self.total_distance).max(if self.camera_status_target.is_some() {
                        1.
                    } else {
                        0.
                    });
            }
        } else if output.speed_limit > 0. && camera && self.camera_status_target.is_some() {
            if let Some(target) = self.camera_status_target {
                self.speed_limit_distance = target;
                output.speed_limit_distance = (target - self.total_distance).max(1.);
            }
        } else if output.speed_limit > 0. && camera {
            if distance_time_changed || self.speed_limit_distance <= self.total_distance {
                self.speed_limit_distance =
                    self.total_distance + output.speed_limit * self.distance_time;
            }
            self.speed_limit_distance = self.speed_limit_distance.max(self.total_distance + 1.);
            output.speed_limit_distance = self.speed_limit_distance - self.total_distance;
        } else {
            self.speed_limit_distance = self.total_distance;
            output.speed_limit_distance = 0.;
        }
    }
}
