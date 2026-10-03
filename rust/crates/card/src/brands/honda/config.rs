use super::{Error, BOSCH, BOSCH_RADARLESS};
use crate::vehicle_params;
use openpilot_cereal::car_capnp::car_params::{self, TransmissionType};

pub struct Config {
    pub candidate: String,
    pub flags: u32,
    pub static_flags: u32,
    pub longitudinal: bool,
    pub pcm: bool,
    pub bsm: bool,
    pub factor: f64,
    pub transmission: TransmissionType,
    pub bus: Bus,
}
#[derive(Clone, Copy)]
pub struct Bus {
    pub pt: u8,
    pub radar: u8,
    pub camera: u8,
    pub lkas: u8,
}
impl Bus {
    pub fn new(offset: u8, flags: u32, longitudinal: bool) -> Self {
        if flags & BOSCH != 0 && flags & BOSCH_RADARLESS == 0 {
            Self {
                pt: offset + 1,
                radar: offset,
                camera: offset + 2,
                lkas: offset + u8::from(longitudinal),
            }
        } else {
            Self {
                pt: offset,
                radar: offset + 1,
                camera: offset + 2,
                lkas: offset,
            }
        }
    }
}
impl Config {
    pub fn new(cp: car_params::Reader<'_>) -> Result<Self, Error> {
        let candidate = cp.get_car_fingerprint()?.to_str()?;
        let platform = vehicle_params::platform(candidate)?;
        if platform.brand != "honda" {
            return Err(Error::Platform(candidate.to_owned()));
        }
        let count = cp.get_safety_configs()?.len();
        let offset = count
            .checked_sub(1)
            .and_then(|n| n.checked_mul(4))
            .and_then(|n| u8::try_from(n).ok())
            .ok_or(Error::Numeric)?;
        let longitudinal = cp.get_openpilot_longitudinal_control();
        Ok(Self {
            candidate: candidate.to_owned(),
            flags: cp.get_flags(),
            static_flags: platform.flags,
            longitudinal,
            pcm: cp.get_pcm_cruise(),
            bsm: cp.get_enable_bsm(),
            factor: f64::from(cp.get_wheel_speed_factor()),
            transmission: cp.get_transmission_type()?,
            bus: Bus::new(offset, platform.flags, longitudinal),
        })
    }
    pub const fn bosch(&self) -> bool {
        self.static_flags & BOSCH != 0
    }
    pub const fn radarless(&self) -> bool {
        self.static_flags & BOSCH_RADARLESS != 0
    }
    pub fn conversion(&self, metric: bool) -> f64 {
        if self.radarless() && !metric {
            1.609344 * (1. / 3.6)
        } else {
            1. / 3.6
        }
    }
}
