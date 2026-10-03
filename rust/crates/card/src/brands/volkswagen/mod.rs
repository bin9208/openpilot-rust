//! Native port of the source Volkswagen PQ/MQB/MEB interface boundaries.
mod can;
mod can_acc;
mod can_aux;
mod can_buttons;
mod can_hud;
mod config;
mod controller;
mod controller_history;
mod controller_hud;
mod controller_long;
mod controller_navi;
mod controller_steering;
mod parameters;
mod runtime;
mod state;
mod state_access;
mod state_buttons;
mod state_data;
mod state_meb;
mod state_meb_controls;
mod state_mqb;
mod state_pq;
mod state_update;
pub use parameters::{parameters, ParamsInput};
pub use runtime::{Setup, Volkswagen};
pub const STOCK_HCA_PRESENT: u32 = 1;
pub const PQ: u32 = 2;
pub const MEB: u32 = 16;
pub const ALT_GEAR: u32 = 32;
pub const STOCK_KLR_PRESENT: u32 = 64;
pub const MEB_GEN2: u32 = 128;
pub const MQB_EVO: u32 = 256;
pub const STOCK_EA_PRESENT: u32 = 16384;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Can(#[from] openpilot_can::Error),
    #[error(transparent)]
    Schema(#[from] capnp::Error),
    #[error(transparent)]
    Enum(#[from] capnp::NotInSchema),
    #[error(transparent)]
    Utf8(#[from] std::str::Utf8Error),
    #[error(transparent)]
    Baseline(#[from] crate::vehicle_params::Error),
    #[error(transparent)]
    Policy(#[from] openpilot_control_policy::Error),
    #[error("Volkswagen numeric conversion")]
    Numeric,
    #[error("unknown Volkswagen platform: {0}")]
    Platform(String),
    #[error("Volkswagen stock message unavailable: {0}")]
    Stock(&'static str),
    #[error("inherited Volkswagen {module} missing function: {function}")]
    SourceFunction {
        module: &'static str,
        function: &'static str,
    },
    #[error("missing Volkswagen copied signal or definition: {0}")]
    Signal(String),
    #[error("inherited Volkswagen MQB first-update failure: name 'np' is not defined")]
    InheritedMqbNumpy,
}
