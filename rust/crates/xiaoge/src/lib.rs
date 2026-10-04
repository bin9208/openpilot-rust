#![forbid(unsafe_code)]

pub mod config;
#[cfg(feature = "native-skip-miri")]
pub mod inference;
pub mod lane;
#[cfg(feature = "native-skip-miri")]
pub mod native;
mod numbers;
pub mod nv12;
pub mod service;
pub mod settings;
pub mod vasm;
pub mod vision;
pub mod wire;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(&'static str),
    #[error("cannot convert float infinity to integer")]
    FloatOverflow,
    #[error("int too large to convert to float")]
    IntegerFloatOverflow,
}
