pub struct Car {
    pub fingerprint: String,
    pub ratio: f64,
    pub globals: [f64; 6],
}
pub struct Pose {
    pub time: f64,
    pub angular: [f64; 3],
    pub angular_std: [f64; 3],
    pub yaw_valid: bool,
    pub roll: f64,
    pub roll_std: f64,
    pub sensors_ok: bool,
    pub posenet_ok: bool,
}
pub enum Input {
    Pose(Pose),
    Calibration([f64; 3]),
    Car {
        speed: f64,
        steering: f64,
    },
    Gps {
        fix: bool,
        latitude: f64,
        longitude: f64,
        bearing: f64,
    },
}
pub struct Event {
    pub time: f64,
    pub input: Input,
}
pub struct Parameters {
    pub valid: bool,
    pub sensor_valid: bool,
    pub ratio_valid: bool,
    pub stiffness_valid: bool,
    pub average_valid: bool,
    pub offset_valid: bool,
    pub estimate_valid: bool,
    pub x: [f64; 9],
    pub std: [f64; 9],
    pub average: f64,
    pub offset: f64,
    pub roll: f64,
    pub debug: bool,
}
pub fn diagonal(values: &[f64]) -> Vec<f64> {
    let mut out = vec![0.; values.len() * values.len()];
    for (i, &value) in values.iter().enumerate() {
        out[i * values.len() + i] = value;
    }
    out
}
pub fn clip(value: f64, low: f64, high: f64) -> f64 {
    if value.is_nan() || low.is_nan() || high.is_nan() {
        f64::NAN
    } else if value < low {
        low
    } else if value > high {
        high
    } else {
        value
    }
}
pub fn hysteresis(valid: bool, value: f64, threshold: f64, lowered: f64) -> bool {
    value.abs() < if valid { threshold } else { lowered }
}
