#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sensor {
    Acceleration,
    Gyroscope,
}
impl Sensor {
    pub fn name(self) -> &'static str {
        match self {
            Self::Acceleration => "accelerometer",
            Self::Gyroscope => "gyroscope",
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub enum Input {
    Sensor {
        kind: Sensor,
        time: f64,
        secondary: bool,
        values: [f64; 3],
    },
    Speed(f64),
    Calibration(Option<[f64; 3]>),
    Camera {
        time: f64,
        rotation: [f64; 3],
        translation: [f64; 3],
        rotation_std: [f64; 3],
        translation_std: [f64; 3],
    },
    Ignored,
}
pub struct Event {
    pub log_time_ns: u64,
    pub valid: bool,
    pub service: &'static str,
    pub input: Input,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[repr(i32)]
pub enum HandleResult {
    Success = 0,
    TimingInvalid = 1,
    InputInvalid = 2,
    SensorSourceInvalid = 3,
}
pub struct Pose {
    pub x: [f64; 18],
    pub std: [f64; 18],
    pub observations: [[f64; 3]; 4],
    pub errors: [[f64; 3]; 4],
    pub debug: bool,
    pub timestamp: u64,
    pub filter_valid: bool,
    pub sensors_valid: bool,
    pub inputs_valid: bool,
    pub posenet_valid: bool,
}
pub const KINDS: [i32; 4] = [10, 4, 14, 13];

pub fn filter_timestamp(time: f64) -> Result<u64, crate::Error> {
    let nanoseconds = if time.is_nan() {
        0.0
    } else {
        (time * 1e9).trunc()
    };
    if !(0.0..u64::MAX as f64).contains(&nanoseconds) {
        return Err(crate::Error::Contract(
            "filter timestamp outside UInt64 range",
        ));
    }
    Ok(nanoseconds as u64)
}

pub fn diagonal<const N: usize, const M: usize>(values: [f64; N]) -> [f64; M] {
    assert_eq!(M, N * N);
    let mut matrix = [0.0; M];
    for (i, value) in values.into_iter().enumerate() {
        matrix[i * N + i] = value;
    }
    matrix
}
