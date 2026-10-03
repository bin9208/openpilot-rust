use super::Error;
use crate::{
    alerts::enum_wire,
    car_specific::{Brand, CarParams},
};
use openpilot_cereal::car_capnp::{car_params, car_params::SafetyModel};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize)]
pub struct SafetyConfig {
    #[serde(with = "enum_wire")]
    pub model: SafetyModel,
    pub parameter: u16,
}
#[derive(Clone, Serialize)]
pub struct Config {
    pub car: CarParams,
    pub flags: u32,
    pub alpha_longitudinal_available: bool,
    pub not_car: bool,
    pub passive: bool,
    pub sec_oc_required: bool,
    pub sec_oc_key_available: bool,
    pub alternative_experience: i16,
    pub safety: Vec<SafetyConfig>,
}
impl Config {
    pub fn read(cp: car_params::Reader<'_>) -> Result<Self, Error> {
        Ok(Self {
            car: CarParams::read(cp)?,
            flags: cp.get_flags(),
            alpha_longitudinal_available: cp.get_alpha_longitudinal_available(),
            not_car: cp.get_not_car(),
            passive: cp.get_passive(),
            sec_oc_required: cp.get_sec_oc_required(),
            sec_oc_key_available: cp.get_sec_oc_key_available(),
            alternative_experience: cp.get_alternative_experience(),
            safety: cp
                .get_safety_configs()?
                .iter()
                .map(|value| {
                    Ok(SafetyConfig {
                        model: value.get_safety_model()?,
                        parameter: value.get_safety_param(),
                    })
                })
                .collect::<Result<_, Error>>()?,
        })
    }
    pub fn recognized(&self) -> bool {
        !matches!(self.car.brand, Brand::Mock)
    }
}
#[derive(Clone, Deserialize, Serialize)]
pub struct Mode {
    pub replay: bool,
    pub simulation: bool,
    pub testing_closet: bool,
    pub device_type: String,
    pub nvme_present: bool,
    pub branch: String,
}
