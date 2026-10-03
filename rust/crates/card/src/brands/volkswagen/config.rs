use super::{Error, MEB, PQ};
use crate::vehicle_params;
use openpilot_cereal::car_capnp::car_params::{self, NetworkLocation, TransmissionType};
use serde::Serialize;
#[derive(Clone, Copy, Serialize)]
pub enum Family {
    Pq,
    Mqb,
    Meb,
}
pub struct Config {
    pub candidate: String,
    pub flags: u32,
    pub family: Family,
    pub network: NetworkLocation,
    pub transmission: TransmissionType,
    pub longitudinal: bool,
    pub pcm: bool,
    pub bsm: bool,
    pub wheel_factor: f64,
    pub starting_speed: f64,
    pub stopping_speed: f64,
}
impl Config {
    pub fn new(cp: car_params::Reader<'_>) -> Result<Self, Error> {
        let candidate = cp.get_car_fingerprint()?.to_str()?;
        if vehicle_params::platform(candidate)?.brand != "volkswagen" {
            return Err(Error::Platform(candidate.to_owned()));
        }
        let flags = cp.get_flags();
        Ok(Self {
            candidate: candidate.to_owned(),
            flags,
            family: if flags & PQ != 0 {
                Family::Pq
            } else if flags & MEB != 0 {
                Family::Meb
            } else {
                Family::Mqb
            },
            network: cp.get_network_location()?,
            transmission: cp.get_transmission_type()?,
            longitudinal: cp.get_openpilot_longitudinal_control(),
            pcm: cp.get_pcm_cruise(),
            bsm: cp.get_enable_bsm(),
            wheel_factor: f64::from(cp.get_wheel_speed_factor()),
            starting_speed: f64::from(cp.get_v_ego_starting()),
            stopping_speed: f64::from(cp.get_v_ego_stopping()),
        })
    }
    pub const fn external_bus(&self) -> u8 {
        match self.network {
            NetworkLocation::FwdCamera => 0,
            NetworkLocation::Gateway => 2,
        }
    }
    pub const fn driver_allowance(&self) -> f64 {
        match self.family {
            Family::Pq | Family::Mqb => 80.,
            Family::Meb => 60.,
        }
    }
    pub const fn ldw_step(&self) -> u64 {
        match self.family {
            Family::Pq => 5,
            Family::Mqb | Family::Meb => 10,
        }
    }
    pub const fn hud_step(&self) -> u64 {
        match self.family {
            Family::Pq => 4,
            Family::Mqb | Family::Meb => 6,
        }
    }
}
