use crate::query::Target;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Ecu {
    #[default]
    Eps,
    Abs,
    FwdRadar,
    FwdCamera,
    Engine,
    Unknown,
    Dsu,
    ParkingAdas,
    Transmission,
    Srs,
    Gateway,
    Hud,
    CombinationMeter,
    Vsa,
    ProgrammedFuelInjection,
    ElectricBrakeBooster,
    ShiftByWire,
    Debug,
    Hybrid,
    Adas,
    Hvac,
    CornerRadar,
    Epb,
    Telematics,
    Body,
}
impl Ecu {
    pub const fn essential(self) -> bool {
        matches!(
            self,
            Self::Engine | Self::Eps | Self::Abs | Self::FwdRadar | Self::FwdCamera | Self::Vsa
        )
    }
    pub const fn fuzzy_shared(self) -> bool {
        matches!(
            self,
            Self::FwdCamera | Self::FwdRadar | Self::Eps | Self::Debug
        )
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Brand {
    Body,
    Chrysler,
    Ford,
    Gm,
    Honda,
    Hyundai,
    Mazda,
    Mock,
    Nissan,
    Psa,
    Rivian,
    Subaru,
    Tesla,
    Toyota,
    Volkswagen,
}
impl Brand {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Body => "body",
            Self::Chrysler => "chrysler",
            Self::Ford => "ford",
            Self::Gm => "gm",
            Self::Honda => "honda",
            Self::Hyundai => "hyundai",
            Self::Mazda => "mazda",
            Self::Mock => "mock",
            Self::Nissan => "nissan",
            Self::Psa => "psa",
            Self::Rivian => "rivian",
            Self::Subaru => "subaru",
            Self::Tesla => "tesla",
            Self::Toyota => "toyota",
            Self::Volkswagen => "volkswagen",
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Firmware {
    pub ecu: Ecu,
    pub address: u32,
    pub sub_address: u8,
    pub fw_version: Vec<u8>,
    pub response_address: u32,
    pub request: Vec<Vec<u8>>,
    pub brand: String,
    pub bus: u8,
    pub logging: bool,
    pub obd_multiplexing: bool,
}

#[derive(Debug, Deserialize)]
pub struct Expected {
    pub ecu: Ecu,
    pub address: u32,
    pub subaddress: Option<u8>,
    pub versions: Vec<Vec<u8>>,
}
impl Expected {
    pub const fn target(&self) -> Target {
        Target(self.address, self.subaddress)
    }
}

#[derive(Debug, Deserialize)]
pub struct Model {
    pub name: String,
    pub brand: Brand,
    pub firmware: Vec<Expected>,
    pub wmis: Vec<String>,
    pub chassis: Vec<String>,
    pub lines: Vec<String>,
    pub years: Vec<String>,
    pub fuzzy_allowed: bool,
}

#[derive(Debug, Deserialize)]
pub struct Request {
    pub request: Vec<Vec<u8>>,
    pub response: Vec<Vec<u8>>,
    pub whitelist: Vec<Ecu>,
    pub offset: i64,
    pub bus: u8,
    pub logging: bool,
    pub obd_multiplexing: bool,
}

#[derive(Debug, Deserialize)]
pub struct BrandConfig {
    pub brand: Brand,
    pub requests: Vec<Request>,
    pub nonessential: Vec<(Ecu, Vec<String>)>,
    pub extra: Vec<Expected>,
    pub fuzzy: bool,
}

#[derive(Debug, Deserialize)]
pub struct Catalog {
    pub brands: Vec<BrandConfig>,
    pub models: Vec<Model>,
    pub selected: Vec<(String, String)>,
}

pub type Live = BTreeMap<Target, BTreeSet<Vec<u8>>>;
pub struct MatchOptions {
    pub exact: bool,
    pub fuzzy: bool,
}
#[derive(Debug, Serialize)]
pub struct CarMatch {
    pub exact: bool,
    pub candidates: BTreeSet<String>,
}
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("firmware catalog missing brand configuration")]
    Brand,
}
