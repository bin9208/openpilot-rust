use super::Error;
use crate::{
    core::Message,
    firmware::Firmware,
    vehicle_params::{self, FinishOptions, TorqueOptions},
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
    if input.candidate != "RIVIAN_R1_GEN1" {
        return Err(Error::Platform(input.candidate.to_owned()));
    }
    let mut message = vehicle_params::baseline(input.candidate)?;
    let mut cp = message.get_root::<car_params::Builder>()?;
    cp.set_brand("rivian");
    let mut safety = cp.reborrow().init_safety_configs(1).get(0);
    safety.set_safety_model(SafetyModel::Rivian);
    safety.set_safety_param(u16::from(input.alpha_long));
    cp.set_steer_actuator_delay(0.15);
    cp.set_steer_limit_timer(0.4);
    vehicle_params::configure_torque(
        input.candidate,
        cp.reborrow().get_lateral_tuning(),
        TorqueOptions::default(),
    )?;
    cp.set_steer_control_type(SteerControlType::Torque);
    cp.set_radar_unavailable(true);
    cp.set_alpha_longitudinal_available(false);
    if input.alpha_long {
        cp.set_openpilot_longitudinal_control(true);
    }
    cp.set_longitudinal_actuator_delay(0.35);
    cp.set_v_ego_stopping(0.25);
    cp.set_stop_accel(0.);
    vehicle_params::finish(
        cp,
        input.settings,
        FinishOptions {
            firmware: input.firmware,
        },
    )?;
    Ok(message)
}
