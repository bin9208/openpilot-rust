use super::{Card, Error, StepIo};
use capnp::message::Builder;
use num_traits::ToPrimitive;
use openpilot_cereal::{
    car_capnp::{car_control, car_params, car_state},
    log_capnp::event,
};

fn time(io: &mut impl StepIo) -> Result<u64, Error> {
    (io.now() * 1e9).to_u64().ok_or(Error::Numeric)
}
pub(super) fn publish(
    card: &Card,
    state: car_state::Reader<'_>,
    io: &mut impl StepIo,
) -> Result<(), Error> {
    if io.subscribers().frame() % 5000 == 0 {
        let mut message = Builder::new_default();
        let mut event = message.init_root::<event::Builder>();
        event.set_valid(true);
        event.set_log_mono_time(time(io)?);
        event.set_car_params(card.params.get_root_as_reader::<car_params::Reader>()?)?;
        io.publish(
            "carParams",
            &capnp::serialize::write_message_to_words(&message),
        )?;
    }
    let mut message = Builder::new_default();
    let mut event = message.init_root::<event::Builder>();
    event.set_valid(io.subscribers().all_checks(&["carControl"])?);
    event.set_log_mono_time(time(io)?);
    event.init_car_output().set_actuators_output(
        card.last_actuators
            .get_root_as_reader::<car_control::actuators::Reader>()?,
    )?;
    io.publish(
        "carOutput",
        &capnp::serialize::write_message_to_words(&message),
    )?;
    let mut message = Builder::new_default();
    let mut event = message.init_root::<event::Builder>();
    event.set_valid(state.get_can_valid());
    event.set_log_mono_time(time(io)?);
    event.set_car_state(state)?;
    let event::Which::CarState(state) = event.which()? else {
        return Err(Error::Event("carState"));
    };
    let mut state = state?;
    state.set_can_error_counter(u32::try_from(card.can_timeouts)?);
    state.set_cum_lag_ms((-card.remaining * 1000.).to_f32().ok_or(Error::Numeric)?);
    io.publish(
        "carState",
        &capnp::serialize::write_message_to_words(&message),
    )?;
    Ok(())
}

pub(super) fn event_time(io: &mut impl StepIo) -> Result<u64, Error> {
    time(io)
}
