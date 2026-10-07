use crate::{
    longitudinal_maneuvers::{Command, Input},
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
pub const OUTPUTS: [&str; 3] = ["alertDebug", "longitudinalPlan", "driverAssistance"];

pub fn stopping_speed(bytes: &[u8]) -> Result<f64, Error> {
    let message = serialize::read_message(bytes, ReaderOptions::new())?;
    Ok(f64::from(
        message
            .get_root::<car_params::Reader<'_>>()?
            .get_v_ego_stopping(),
    ))
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
    Ok(Input {
        speed: f64::from(car.get_v_ego()),
        active: control.get_long_active(),
        standstill: car.get_standstill(),
        cruise_standstill: car.get_cruise_state()?.get_standstill(),
        valid: state.all_checks(&[])?,
    })
}

pub fn encode(
    command: &Command,
    valid: bool,
    mut clock: impl FnMut() -> Result<u64, Error>,
) -> Result<[Vec<u8>; 3], Error> {
    let mut alert = Builder::new_default();
    let mut root = alert.init_root::<event::Builder<'_>>();
    root.set_log_mono_time(clock()?);
    root.set_valid(true);
    let mut data = root.init_alert_debug();
    data.set_alert_text1(command.alert_text1.as_str());
    if command.selected.is_some() {
        data.set_alert_text2(command.alert_text2);
    }
    let mut plan = Builder::new_default();
    let mut root = plan.init_root::<event::Builder<'_>>();
    root.set_log_mono_time(clock()?);
    root.set_valid(valid);
    let mut data = root.init_longitudinal_plan();
    data.set_a_target(command.acceleration as f32);
    data.set_should_stop(command.should_stop);
    data.set_allow_brake(true);
    data.set_allow_throttle(true);
    data.set_has_lead(true);
    data.init_speeds(1).set(0, 0.2);
    let mut assistance = Builder::new_default();
    let mut root = assistance.init_root::<event::Builder<'_>>();
    root.set_log_mono_time(clock()?);
    root.set_valid(true);
    root.init_driver_assistance();
    Ok([
        serialize::write_message_to_words(&alert),
        serialize::write_message_to_words(&plan),
        serialize::write_message_to_words(&assistance),
    ])
}
