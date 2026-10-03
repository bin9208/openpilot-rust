mod can;
mod controller;
mod parameters;
mod runtime;
mod state;

pub use parameters::{parameters, ParamsInput};
pub use runtime::{Mazda, Setup};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Can(#[from] openpilot_can::Error),
    #[error(transparent)]
    Schema(#[from] capnp::Error),
    #[error(transparent)]
    Enum(#[from] capnp::NotInSchema),
    #[error(transparent)]
    Baseline(#[from] crate::vehicle_params::Error),
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error("Mazda numeric conversion")]
    Numeric,
    #[error("Mazda GEN1 button frame is unavailable for flags {0}")]
    ButtonFlags(u32),
}
