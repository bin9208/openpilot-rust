use openpilot_calibrationd::orientation::{compose, rotation};
use openpilot_locationd::orientation::{apply, rotate_std, Matrix, IDENTITY};
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct Measurement {
    pub xyz: [f64; 3],
    pub xyz_std: [f64; 3],
}

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct Pose {
    pub orientation: Measurement,
    pub velocity: Measurement,
    pub acceleration: Measurement,
    pub angular_velocity: Measurement,
}

#[derive(Serialize)]
pub struct PoseCalibrator {
    pub calibrated: bool,
    rpy: [f64; 3],
    calib_from_device: Matrix,
}

impl Default for PoseCalibrator {
    fn default() -> Self {
        Self {
            calibrated: false,
            rpy: [0.0; 3],
            calib_from_device: IDENTITY,
        }
    }
}

impl PoseCalibrator {
    pub fn feed(&mut self, rpy: [f64; 3], calibrated: bool) {
        self.rpy = rpy;
        let matrix = rotation(rpy);
        self.calib_from_device = std::array::from_fn(|i| std::array::from_fn(|j| matrix[j][i]));
        self.calibrated = calibrated;
    }

    fn transform(&self, input: &Measurement) -> Measurement {
        Measurement {
            xyz: apply(self.calib_from_device, input.xyz),
            xyz_std: rotate_std(self.calib_from_device, input.xyz_std),
        }
    }

    pub fn build(&self, input: &Pose) -> Pose {
        Pose {
            orientation: Measurement {
                xyz: compose(input.orientation.xyz, self.rpy),
                xyz_std: [f64::NAN; 3],
            },
            velocity: self.transform(&input.velocity),
            acceleration: self.transform(&input.acceleration),
            angular_velocity: self.transform(&input.angular_velocity),
        }
    }
}

#[derive(Default, Deserialize)]
pub struct ActuationInput {
    pub longitudinal_active: bool,
    pub lateral_active: bool,
    pub steering_pressed: bool,
    pub ego_acceleration: f64,
    pub ego_speed: f64,
    pub roll: f64,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ExcessiveActuation {
    Longitudinal,
    Lateral,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("math domain error")]
    InfiniteRoll,
}

#[derive(Default, Serialize)]
pub struct ExcessiveActuationCheck {
    excessive_counter: u64,
    engaged_counter: u64,
}

impl ExcessiveActuationCheck {
    pub fn update(
        &mut self,
        input: &ActuationInput,
        pose: &Pose,
    ) -> Result<Option<ExcessiveActuation>, Error> {
        let acceleration = pose.acceleration.xyz[0];
        let excessive_acceleration = acceleration > 2.5 * 2.0;
        let excessive_deceleration = acceleration < -4.0 * 2.0;
        let longitudinal =
            input.longitudinal_active && (excessive_acceleration || excessive_deceleration);
        if input.roll.is_infinite() {
            return Err(Error::InfiniteRoll);
        }
        let lateral_acceleration =
            input.ego_speed * pose.angular_velocity.xyz[2] - input.roll.sin() * 9.81;
        self.engaged_counter = if input.lateral_active && !input.steering_pressed {
            self.engaged_counter + 1
        } else {
            0
        };
        let lateral = self.engaged_counter > 100 && lateral_acceleration.abs() > 3.0 * 2.0;
        let valid = (input.ego_acceleration - acceleration).abs() < 2.0;
        self.excessive_counter = if valid && (longitudinal || lateral) {
            self.excessive_counter + 1
        } else {
            0
        };
        Ok(if self.excessive_counter > 25 {
            Some(if longitudinal {
                ExcessiveActuation::Longitudinal
            } else {
                ExcessiveActuation::Lateral
            })
        } else {
            None
        })
    }
}

pub fn camera_packets(
    use_wide_camera: bool,
    disable_dm: i32,
    simulation: bool,
) -> Vec<&'static str> {
    let mut packets = vec!["roadCameraState"];
    if disable_dm == 0 || simulation {
        packets.push("driverCameraState");
    }
    if use_wide_camera {
        packets.push("wideRoadCameraState");
    }
    packets
}
