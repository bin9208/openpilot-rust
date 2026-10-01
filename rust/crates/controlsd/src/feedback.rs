use crate::{
    controller::{Command, Controls},
    inputs::{Inputs, Pose},
};

impl Controls {
    pub fn update_pose(&mut self, input: &Inputs) {
        if let Some(calibration) = input.calibration {
            self.calibration = calibration;
        }
        if let Some(pose) = &input.pose {
            let rotation = openpilot_calibrationd::orientation::rotation(self.calibration);
            self.pose = Some(Pose {
                orientation: openpilot_calibrationd::orientation::compose(
                    pose.orientation,
                    self.calibration,
                ),
                angular: std::array::from_fn(|i| {
                    rotation[0][i] * pose.angular[0]
                        + rotation[1][i] * pose.angular[1]
                        + rotation[2][i] * pose.angular[2]
                }),
            });
        }
    }
    pub fn feedback(&mut self, input: &Inputs, command: &Command) {
        if input.selfdrive.active {
            self.safety_limited = if self.config.meb() {
                (f64::from(command.curvature) - input.output_curvature).abs()
                    * input.car.speed.powi(2)
                    > 0.1
            } else if self.config.angle {
                (f64::from(command.angle) - input.output_angle).abs() > 3.
            } else {
                (f64::from(command.torque) - input.output_torque).abs() > 1e-2
            };
        }
    }
}
