use super::Error;
use crate::{
    core::Message,
    firmware::Firmware,
    vehicle_params::{self, FinishOptions, TorqueOptions},
};
use openpilot_cereal::car_capnp::car_params::{self, SafetyModel};
use openpilot_params::Params;

pub struct ParamsInput<'a> {
    pub candidate: &'a str,
    pub fingerprints: &'a [(u8, Vec<(u32, usize)>)],
    pub firmware: &'a [Firmware],
    pub alpha_long: bool,
    pub settings: &'a Params,
}

pub fn parameters(input: ParamsInput<'_>) -> Result<Message, Error> {
    let mut message = vehicle_params::baseline(input.candidate)?;
    let mut cp = message.get_root::<car_params::Builder>()?;
    cp.set_brand("mazda");
    cp.reborrow()
        .init_safety_configs(1)
        .get(0)
        .set_safety_model(SafetyModel::Mazda);
    cp.set_radar_unavailable(true);
    cp.set_dashcam_only(!matches!(
        input.candidate,
        "MAZDA_CX5_2022" | "MAZDA_CX9_2021"
    ));
    cp.set_steer_actuator_delay(0.1);
    cp.set_steer_limit_timer(0.8);
    vehicle_params::configure_torque(
        input.candidate,
        cp.reborrow().get_lateral_tuning(),
        TorqueOptions::default(),
    )?;
    if input.candidate != "MAZDA_CX5_2022" {
        cp.set_min_steer_speed((45. * (1. / 3.6)) as f32);
    }
    let wheelbase = f64::from(cp.reborrow_as_reader().get_wheelbase());
    cp.set_center_to_front((wheelbase * 0.41) as f32);
    vehicle_params::finish(
        cp,
        input.settings,
        FinishOptions {
            firmware: input.firmware,
        },
    )?;
    Ok(message)
}
