mod can;
mod controller;
mod parameters;
mod runtime;
mod state;

pub use parameters::{parameters, ParamsInput};
pub use runtime::{Chrysler, Setup};

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
    #[error("Chrysler numeric conversion")]
    Numeric,
    #[error("unknown Chrysler platform: {0}")]
    Platform(String),
}

#[derive(Clone, Copy)]
enum Candidate {
    Pacifica2018Hybrid,
    Pacifica2019Hybrid,
    Pacifica2018,
    Pacifica2020,
    Durango,
    GrandCherokee,
    GrandCherokee2019,
    RamDt,
    RamHd,
}
impl TryFrom<&str> for Candidate {
    type Error = Error;
    fn try_from(value: &str) -> Result<Self, Error> {
        match value {
            "CHRYSLER_PACIFICA_2018_HYBRID" => Ok(Self::Pacifica2018Hybrid),
            "CHRYSLER_PACIFICA_2019_HYBRID" => Ok(Self::Pacifica2019Hybrid),
            "CHRYSLER_PACIFICA_2018" => Ok(Self::Pacifica2018),
            "CHRYSLER_PACIFICA_2020" => Ok(Self::Pacifica2020),
            "DODGE_DURANGO" => Ok(Self::Durango),
            "JEEP_GRAND_CHEROKEE" => Ok(Self::GrandCherokee),
            "JEEP_GRAND_CHEROKEE_2019" => Ok(Self::GrandCherokee2019),
            "RAM_1500_5TH_GEN" => Ok(Self::RamDt),
            "RAM_HD_5TH_GEN" => Ok(Self::RamHd),
            other => Err(Error::Platform(other.to_owned())),
        }
    }
}
impl Candidate {
    fn ram(self) -> bool {
        matches!(self, Self::RamDt | Self::RamHd)
    }
    fn higher_min(self) -> bool {
        matches!(
            self,
            Self::Pacifica2019Hybrid | Self::Pacifica2020 | Self::GrandCherokee2019 | Self::Durango
        )
    }
    fn dbc(self) -> &'static str {
        match self {
            Self::RamDt => "chrysler_ram_dt_generated.dbc",
            Self::RamHd => "chrysler_ram_hd_generated.dbc",
            Self::Pacifica2018Hybrid
            | Self::Pacifica2019Hybrid
            | Self::Pacifica2018
            | Self::Pacifica2020
            | Self::Durango
            | Self::GrandCherokee
            | Self::GrandCherokee2019 => "chrysler_pacifica_2017_hybrid_generated.dbc",
        }
    }
    fn torque_limit(self) -> i32 {
        if matches!(self, Self::RamHd) {
            361
        } else {
            261
        }
    }
    fn torque_delta(self) -> i32 {
        match self {
            Self::RamDt => 6,
            Self::RamHd => 14,
            Self::Pacifica2018Hybrid
            | Self::Pacifica2019Hybrid
            | Self::Pacifica2018
            | Self::Pacifica2020
            | Self::Durango
            | Self::GrandCherokee
            | Self::GrandCherokee2019 => 3,
        }
    }
}
