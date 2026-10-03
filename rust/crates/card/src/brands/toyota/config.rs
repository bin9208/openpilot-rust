use super::{eps_scale, Error};
use crate::vehicle_params;
use openpilot_cereal::car_capnp::car_params::{self, SteerControlType};

pub struct Config {
    pub candidate: String,
    pub flags: u32,
    pub static_flags: u32,
    pub eps_scale: f64,
    pub wheel_factor: f64,
    pub angle: bool,
    pub bsm: bool,
    pub dsu: bool,
    pub longitudinal: bool,
    pub pcm: bool,
}
impl Config {
    pub fn new(cp: car_params::Reader<'_>) -> Result<Self, Error> {
        let candidate = cp.get_car_fingerprint()?.to_str()?;
        let platform = vehicle_params::platform(candidate)?;
        if platform.brand != "toyota" {
            return Err(Error::Platform(candidate.to_owned()));
        }
        Ok(Self {
            candidate: candidate.to_owned(),
            flags: cp.get_flags(),
            static_flags: platform.flags,
            eps_scale: f64::from(eps_scale(candidate)) / 100.,
            wheel_factor: f64::from(cp.get_wheel_speed_factor()),
            angle: cp.get_steer_control_type()? == SteerControlType::Angle,
            bsm: cp.get_enable_bsm(),
            dsu: cp.get_enable_dsu(),
            longitudinal: cp.get_openpilot_longitudinal_control(),
            pcm: cp.get_pcm_cruise(),
        })
    }
}
