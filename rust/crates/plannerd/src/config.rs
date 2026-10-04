use crate::{lateral_planner, longitudinal_planner, Error};
use capnp::{message::ReaderOptions, serialize};
use openpilot_cereal::car_capnp::car_params;
use openpilot_control_policy::math::maximum;

pub struct Config {
    pub brand: String,
    pub longitudinal: longitudinal_planner::Vehicle,
    pub lateral: lateral_planner::Vehicle,
    pub radar_unavailable: bool,
    pub front_delay: f64,
}

impl Config {
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        let message = serialize::read_message(bytes, ReaderOptions::new())?;
        let cp = message.get_root::<car_params::Reader<'_>>()?;
        let brand = cp.get_brand()?.to_str()?.to_owned();
        let meb = brand == "volkswagen" && cp.get_flags() & 16 != 0;
        Ok(Self {
            brand,
            longitudinal: longitudinal_planner::Vehicle {
                longitudinal_control: cp.get_openpilot_longitudinal_control(),
                volkswagen_meb: meb,
            },
            lateral: lateral_planner::Vehicle {
                wheelbase: f64::from(cp.get_wheelbase()),
                center_to_front: f64::from(cp.get_center_to_front()),
                mass: f64::from(cp.get_mass()),
                tire_stiffness_rear: f64::from(cp.get_tire_stiffness_rear()),
            },
            radar_unavailable: cp.get_radar_unavailable(),
            front_delay: if meb {
                0.
            } else {
                maximum(0., f64::from(cp.get_radar_delay()))
            },
        })
    }
    pub fn live_tracks(&self, configured_mode: i32) -> bool {
        self.brand == "hyundai" && configured_mode >= 1
    }
}
