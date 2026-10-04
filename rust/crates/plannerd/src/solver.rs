use crate::Error;
use serde::Deserialize;
#[cfg(feature = "native-skip-miri")]
use std::ffi::CStr;

#[cfg(feature = "native-skip-miri")]
mod access;
#[cfg(feature = "native-skip-miri")]
mod api;
#[cfg(feature = "native-skip-miri")]
mod artifact;
#[cfg(feature = "native-skip-miri")]
mod capsule;
#[cfg(feature = "native-skip-miri")]
pub use capsule::Acados;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Lateral,
    Longitudinal,
}

impl Kind {
    pub const fn horizon(self) -> usize {
        match self {
            Self::Lateral => 32,
            Self::Longitudinal => 12,
        }
    }
    pub const fn states(self) -> usize {
        match self {
            Self::Lateral => 4,
            Self::Longitudinal => 3,
        }
    }
    pub const fn parameters(self) -> usize {
        match self {
            Self::Lateral => 2,
            Self::Longitudinal => 8,
        }
    }
    #[cfg(feature = "native-skip-miri")]
    const fn prefix(self) -> &'static str {
        match self {
            Self::Lateral => "lat",
            Self::Longitudinal => "long",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Field {
    State,
    Control,
    Parameters,
    Reference,
    Weights,
    LowerSlack,
    LowerBound,
    UpperBound,
}

impl Field {
    pub fn shape(self, kind: Kind, stage: usize) -> Result<[usize; 2], Error> {
        if stage > kind.horizon() {
            return Err(Error::Contract("solver stage exceeds horizon"));
        }
        let costs = match (kind, stage == kind.horizon()) {
            (Kind::Lateral, false) => 5,
            (Kind::Lateral, true) => 3,
            (Kind::Longitudinal, false) => 6,
            (Kind::Longitudinal, true) => 5,
        };
        match self {
            Self::State => Ok([kind.states(), 0]),
            Self::Control if stage < kind.horizon() => Ok([1, 0]),
            Self::Parameters => Ok([kind.parameters(), 0]),
            Self::Reference => Ok([costs, 0]),
            Self::Weights => Ok([costs, costs]),
            Self::LowerSlack if kind == Kind::Longitudinal && stage < kind.horizon() => Ok([4, 0]),
            Self::LowerBound | Self::UpperBound if stage == 0 => Ok([kind.states(), 0]),
            Self::Control | Self::LowerSlack | Self::LowerBound | Self::UpperBound => {
                Err(Error::Contract("solver field unavailable at this stage"))
            }
        }
    }
    #[cfg(feature = "native-skip-miri")]
    const fn name(self) -> &'static CStr {
        match self {
            Self::State => c"x",
            Self::Control => c"u",
            Self::Parameters => c"p",
            Self::Reference => c"yref",
            Self::Weights => c"W",
            Self::LowerSlack => c"Zl",
            Self::LowerBound => c"lbx",
            Self::UpperBound => c"ubx",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Statistic {
    TotalTime,
    QpTime,
    LinearizationTime,
    IntegratorTime,
}

impl Statistic {
    #[cfg(feature = "native-skip-miri")]
    const fn name(self) -> &'static CStr {
        match self {
            Self::TotalTime => c"time_tot",
            Self::QpTime => c"time_qp",
            Self::LinearizationTime => c"time_lin",
            Self::IntegratorTime => c"time_sim",
        }
    }
}
