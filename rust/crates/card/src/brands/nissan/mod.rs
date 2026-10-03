mod can;
mod controller;
mod parameters;
mod runtime;
mod state;

pub use parameters::{parameters, ParamsInput};
pub use runtime::{Nissan, Setup};

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
    #[error("Nissan numeric conversion")]
    Numeric,
    #[error("unknown Nissan platform: {0}")]
    Platform(String),
    #[error("missing Nissan copied signal: {0}")]
    Signal(String),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Candidate {
    Xtrail,
    Leaf,
    LeafIc,
    Rogue,
    Altima,
}
impl TryFrom<&str> for Candidate {
    type Error = Error;
    fn try_from(value: &str) -> Result<Self, Error> {
        match value {
            "NISSAN_XTRAIL" => Ok(Self::Xtrail),
            "NISSAN_LEAF" => Ok(Self::Leaf),
            "NISSAN_LEAF_IC" => Ok(Self::LeafIc),
            "NISSAN_ROGUE" => Ok(Self::Rogue),
            "NISSAN_ALTIMA" => Ok(Self::Altima),
            other => Err(Error::Platform(other.to_owned())),
        }
    }
}
impl Candidate {
    fn leaf(self) -> bool {
        matches!(self, Self::Leaf | Self::LeafIc)
    }
    fn altima(self) -> bool {
        self == Self::Altima
    }
    fn dbc(self) -> &'static str {
        match self {
            Self::Leaf | Self::LeafIc => "nissan_leaf_2018_generated.dbc",
            Self::Xtrail | Self::Rogue | Self::Altima => "nissan_x_trail_2017_generated.dbc",
        }
    }
}
