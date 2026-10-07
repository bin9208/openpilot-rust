use crate::{
    lateral_maneuvers::{Command, Input},
    Error,
};
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use openpilot_cereal::{car_capnp::car_params, log_capnp::event};
use openpilot_messaging::state::State;

pub const TOPICS: [&str; 5] = [
    "carState",
    "carControl",
    "controlsState",
    "selfdriveState",
    "modelV2",
];
pub const OUTPUTS: [&str; 2] = ["alertDebug", "lateralManeuverPlan"];

pub fn validate_params(bytes: &[u8]) -> Result<(), Error> {
    serialize::read_message(bytes, ReaderOptions::new())?.get_root::<car_params::Reader<'_>>()?;
    Ok(())
}

pub fn input(state: &State) -> Result<Input, Error> {
    let event::Which::CarState(car) = state
        .topic("carState")?
        .event()?
        .which()
        .map_err(|_| Error::Contract("invalid carState union"))?
    else {
        return Err(Error::Contract("unexpected carState event"));
    };
    let car = car?;
    let event::Which::CarControl(control) = state
        .topic("carControl")?
        .event()?
        .which()
        .map_err(|_| Error::Contract("invalid carControl union"))?
    else {
        return Err(Error::Contract("unexpected carControl event"));
    };
    let control = control?;
    let event::Which::ControlsState(controls) = state
        .topic("controlsState")?
        .event()?
        .which()
        .map_err(|_| Error::Contract("invalid controlsState union"))?
    else {
        return Err(Error::Contract("unexpected controlsState event"));
    };
    let controls = controls?;
    Ok(Input {
        speed: f64::from(car.get_v_ego()),
        active: control.get_lat_active(),
        steering_pressed: car.get_steering_pressed(),
        curvature: f64::from(controls.get_desired_curvature()),
        orientation: control
            .get_orientation_n_e_d()?
            .iter()
            .map(f64::from)
            .collect(),
        valid: true,
    })
}

pub fn encode(
    command: &Command,
    mut clock: impl FnMut() -> Result<u64, Error>,
) -> Result<[Vec<u8>; 2], Error> {
    let mut alert = Builder::new_default();
    let mut root = alert.init_root::<event::Builder<'_>>();
    root.set_log_mono_time(clock()?);
    root.set_valid(true);
    let mut data = root.init_alert_debug();
    data.set_alert_text1(command.alert_text1.as_str());
    if let Some(text) = command.alert_text2 {
        data.set_alert_text2(text);
    }
    let mut plan = Builder::new_default();
    let mut root = plan.init_root::<event::Builder<'_>>();
    root.set_log_mono_time(clock()?);
    root.set_valid(command.valid);
    let mut data = root.init_lateral_maneuver_plan();
    if command.valid {
        data.set_desired_curvature(command.curvature as f32);
    }
    Ok([
        serialize::write_message_to_words(&alert),
        serialize::write_message_to_words(&plan),
    ])
}
