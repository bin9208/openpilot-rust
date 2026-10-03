use super::{Candidate, Error};
use crate::{
    core::Message,
    firmware::Firmware,
    vehicle_params::{self, FinishOptions},
};
use openpilot_cereal::car_capnp::car_params::{self, SafetyModel, SteerControlType};
use openpilot_params::Params;

pub struct ParamsInput<'a> {
    pub candidate: &'a str,
    pub fingerprints: &'a [(u8, Vec<(u32, usize)>)],
    pub firmware: &'a [Firmware],
    pub alpha_long: bool,
    pub settings: &'a Params,
}

pub fn parameters(input: ParamsInput<'_>) -> Result<Message, Error> {
    let candidate = Candidate::try_from(input.candidate)?;
    let mut message = vehicle_params::baseline(input.candidate)?;
    let mut cp = message.get_root::<car_params::Builder>()?;
    cp.set_brand("nissan");
    let mut safety = cp.reborrow().init_safety_configs(1).get(0);
    safety.set_safety_model(SafetyModel::Nissan);
    safety.set_safety_param(u16::from(candidate.altima()));
    cp.set_auto_resume_sng(false);
    cp.set_steer_limit_timer(1.);
    cp.set_steer_actuator_delay(0.1);
    cp.set_steer_control_type(SteerControlType::Angle);
    cp.set_radar_unavailable(true);
    vehicle_params::finish(
        cp,
        input.settings,
        FinishOptions {
            firmware: input.firmware,
        },
    )?;
    Ok(message)
}
