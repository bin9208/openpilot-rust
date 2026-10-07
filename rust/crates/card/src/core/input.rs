use super::Error;
use openpilot_cereal::{
    car_capnp::car_control,
    log_capnp::{event, model_data_v2, onroad_event, radar_state},
};
use openpilot_messaging::state::State;

pub(super) fn control(state: &State) -> Result<car_control::Reader<'_>, Error> {
    let event::Which::CarControl(value) = state.topic("carControl")?.event()?.which()? else {
        return Err(Error::Event("carControl"));
    };
    Ok(value?)
}
pub(super) fn initialized(state: &State) -> Result<bool, Error> {
    let topic = state.topic("onroadEvents")?;
    let event::Which::OnroadEvents(events) = topic.event()?.which()? else {
        return Err(Error::Event("onroadEvents"));
    };
    for value in events? {
        if value.get_name()? == onroad_event::EventName::SelfdriveInitializing {
            return Ok(false);
        }
    }
    Ok(topic.seen)
}
pub(super) fn model(state: &State) -> Result<Option<model_data_v2::Reader<'_>>, Error> {
    let topic = state.topic("modelV2")?;
    if !topic.valid || !topic.alive {
        return Ok(None);
    }
    let event::Which::ModelV2(value) = topic.event()?.which()? else {
        return Err(Error::Event("modelV2"));
    };
    Ok(Some(value?))
}
pub(super) fn radar(state: &State) -> Result<Option<radar_state::Reader<'_>>, Error> {
    let topic = state.topic("radarState")?;
    if !topic.valid || !topic.alive {
        return Ok(None);
    }
    let event::Which::RadarState(value) = topic.event()?.which()? else {
        return Err(Error::Event("radarState"));
    };
    Ok(Some(value?))
}
pub(super) fn vision_bytes(state: &State) -> Result<Option<&[u8]>, Error> {
    let topic = state.topic("customReservedRawData0")?;
    if !topic.updated {
        return Ok(None);
    }
    let event::Which::CustomReservedRawData0(bytes) = topic.event()?.which()? else {
        return Err(Error::Event("customReservedRawData0"));
    };
    Ok(Some(bytes?))
}
