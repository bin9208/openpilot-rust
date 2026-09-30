#![forbid(unsafe_code)]
pub mod communication;
pub mod diagnostics;
pub mod runtime;
mod value;
mod value_decode;

pub use value::{Fields, Number, PythonText, Value};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("diagnostic count overflow")]
    CountOverflow,
    #[error(transparent)]
    Round(#[from] std::num::ParseFloatError),
    #[error(transparent)]
    Format(#[from] std::fmt::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Transport(#[from] zmq::Error),
    #[error("{0}")]
    Contract(&'static str),
}

pub mod context;
pub mod record;
pub mod site;

pub mod native;
pub mod producer;
pub mod rate;
