use openpilot_calibrationd::orientation::{compose, rotation};
#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Measurement {
    pub xyz: [f64; 3],
    pub std: [f64; 3],
}
#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Pose {
    pub orientation: Measurement,
    pub velocity: Measurement,
    pub acceleration: Measurement,
    pub angular_velocity: Measurement,
}
#[derive(Debug, serde::Serialize)]
pub struct Calibrator {
    pub valid: bool,
    pub rotation: [[f64; 3]; 3],
    rpy: [f64; 3],
}
impl Default for Calibrator {
    fn default() -> Self {
        Self {
            valid: false,
            rotation: rotation([0.; 3]),
            rpy: [0.; 3],
        }
    }
}
impl Calibrator {
    pub fn feed(&mut self, rpy: [f64; 3], valid: bool) {
        let matrix = rotation(rpy);
        self.rotation = std::array::from_fn(|row| std::array::from_fn(|col| matrix[col][row]));
        self.valid = valid;
        self.rpy = rpy;
    }
    pub fn transform(&self, value: Measurement) -> Measurement {
        let xyz = self
            .rotation
            .map(|row| row[0] * value.xyz[0] + row[1] * value.xyz[1] + row[2] * value.xyz[2]);
        let std = self.rotation.map(|row| {
            ((row[0] * value.std[0].powi(2)) * row[0]
                + (row[1] * value.std[1].powi(2)) * row[1]
                + (row[2] * value.std[2].powi(2)) * row[2])
                .sqrt()
        });
        Measurement { xyz, std }
    }
    pub fn pose(&self, pose: Pose) -> Pose {
        Pose {
            orientation: Measurement {
                xyz: compose(pose.orientation.xyz, self.rpy),
                std: [f64::NAN; 3],
            },
            velocity: self.transform(pose.velocity),
            acceleration: self.transform(pose.acceleration),
            angular_velocity: self.transform(pose.angular_velocity),
        }
    }
}
