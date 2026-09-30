#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("calibration contract: {0}")]
    Contract(&'static str),
    #[error("calibration cereal: {0}")]
    Cereal(#[from] capnp::Error),
    #[error("calibration enum: {0}")]
    Enum(#[from] capnp::NotInSchema),
    #[error("calibration I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("calibration Params: {0}")]
    Params(#[from] openpilot_params::Error),
    #[error("calibration state: {0}")]
    State(#[from] openpilot_messaging::state::Error),
    #[cfg(feature = "native-skip-miri")]
    #[error("calibration IPC: {0}")]
    Messaging(#[from] openpilot_messaging::runtime::Error),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Uncalibrated,
    Calibrated,
    Invalid,
    Recalibrating,
}

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub pitch: [f64; 2],
}

impl Limits {
    pub const fn standard() -> Self {
        Self {
            pitch: [-0.09074112085129739, 0.17],
        }
    }
    pub const fn mici() -> Self {
        Self {
            pitch: [-0.143101, 0.22235988],
        }
    }
    pub fn valid(self, rpy: &[f64]) -> Result<bool, Error> {
        let pitch = *rpy.get(1).ok_or(Error::Contract("saved pitch missing"))?;
        if !(self.pitch[0] < pitch && pitch < self.pitch[1]) {
            return Ok(false);
        }
        let yaw = *rpy.get(2).ok_or(Error::Contract("saved yaw missing"))?;
        Ok(YAW_LIMITS[0] < yaw && yaw < YAW_LIMITS[1])
    }
    pub fn clip(self, rpy: [f64; 3]) -> [f64; 3] {
        let rpy = if rpy.iter().any(|value| value.is_nan()) {
            [0.0; 3]
        } else {
            rpy
        };
        [
            rpy[0],
            rpy[1].clamp(self.pitch[0] - 0.005, self.pitch[1] + 0.005),
            rpy[2].clamp(YAW_LIMITS[0] - 0.005, YAW_LIMITS[1] + 0.005),
        ]
    }
}

pub const YAW_LIMITS: [f64; 2] = [-0.06912048084718224, 0.06912048084718235];
pub const BLOCK_SIZE: u8 = 100;
pub const INPUTS_WANTED: u8 = 50;
pub const INPUTS_NEEDED: u8 = 5;
pub const MIN_SPEED: f64 = 15.0 * (1.609344 * (1.0 / 3.6));

#[derive(Clone, Debug)]
pub struct Seed {
    // Finite malformed saved RPY arrays remain observable until the source would fail.
    pub rpy: Vec<f64>,
    pub valid_blocks: i32,
    pub wide: Vec<f64>,
    pub height: Vec<f64>,
}

impl Default for Seed {
    fn default() -> Self {
        Self {
            rpy: vec![0.0; 3],
            valid_blocks: 0,
            wide: vec![0.0; 3],
            height: vec![1.22],
        }
    }
}

#[derive(Clone, Debug)]
pub struct Odometry {
    pub trans: Vec<f64>,
    pub rot: Vec<f64>,
    pub trans_std: Vec<f64>,
    pub wide: Vec<f64>,
    pub road: Vec<f64>,
    pub road_std: Vec<f64>,
}

pub struct Update {
    pub rpy: Option<[f64; 3]>,
    pub persist: bool,
}
