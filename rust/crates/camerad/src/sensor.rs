mod config;
mod data;
mod registers;
mod score;

pub use config::SensorConfig;
pub use registers::ExposureRegisters;
pub use score::ExposureScore;

use serde::Serialize;
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SensorKind {
    Ar0231,
    Ox03c10,
    Os04c10,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
pub struct Register(pub u32, pub u32);

#[derive(Clone, Copy, Debug)]
pub struct Exposure {
    pub time: i32,
    pub gain_index: i32,
    pub dc_gain: bool,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum SensorError {
    #[error("camera port {0} is outside 0..3")]
    Port(usize),
    #[error("sensor gain index {0} is outside the initialized gain table")]
    GainIndex(i32),
}

impl SensorKind {
    pub const fn config(self) -> &'static SensorConfig {
        match self {
            Self::Ar0231 => &data::AR0231,
            Self::Ox03c10 => &data::OX03C10,
            Self::Os04c10 => &data::OS04C10,
        }
    }

    pub fn slave_address(self, port: usize) -> Result<u32, SensorError> {
        let addresses = match self {
            Self::Ar0231 => [0x20, 0x30, 0x20],
            Self::Ox03c10 | Self::Os04c10 => [0x6c, 0x20, 0x6c],
        };
        addresses.get(port).copied().ok_or(SensorError::Port(port))
    }
}
