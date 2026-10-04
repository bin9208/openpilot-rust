use crate::Error;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PlannerMode {
    Acc,
    Blended,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MpcSource {
    Lead0,
    Lead1,
    Cruise,
    E2e,
}

impl MpcSource {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Lead0 => "lead0",
            Self::Lead1 => "lead1",
            Self::Cruise => "cruise",
            Self::E2e => "e2e",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(try_from = "i32", into = "i32")]
pub enum Personality {
    Aggressive,
    Standard,
    Relaxed,
    MoreRelaxed,
}

impl Personality {
    pub const fn index(self) -> usize {
        match self {
            Self::Aggressive => 0,
            Self::Standard => 1,
            Self::Relaxed => 2,
            Self::MoreRelaxed => 3,
        }
    }
    pub const fn jerk_factor(self) -> f64 {
        match self {
            Self::Aggressive => 0.5,
            Self::Standard => 0.7,
            Self::Relaxed | Self::MoreRelaxed => 1.,
        }
    }
}
impl TryFrom<i32> for Personality {
    type Error = Error;
    fn try_from(value: i32) -> Result<Self, Error> {
        match value {
            0 => Ok(Self::Aggressive),
            1 => Ok(Self::Standard),
            2 => Ok(Self::Relaxed),
            3 => Ok(Self::MoreRelaxed),
            _ => Err(Error::Contract("unsupported longitudinal personality")),
        }
    }
}
impl From<Personality> for i32 {
    fn from(value: Personality) -> Self {
        match value {
            Personality::Aggressive => 0,
            Personality::Standard => 1,
            Personality::Relaxed => 2,
            Personality::MoreRelaxed => 3,
        }
    }
}
