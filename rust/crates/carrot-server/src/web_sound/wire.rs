use super::{clock, Button};
use crate::Error;
use openpilot_cereal::log_capnp::event;
use openpilot_messaging::state::State;

pub(super) struct Snapshot {
    pub now: f64,
    pub received: f64,
    pub valid: bool,
    pub enabled: bool,
    pub alert: u16,
    pub updated: bool,
    pub countdown: i32,
    pub countdown_valid: bool,
    pub countdown_updated: bool,
    pub car_valid: bool,
    pub buttons: Vec<Button>,
}

fn cereal(error: capnp::Error) -> Error {
    Error::Source(error.to_string())
}

pub(super) fn snapshot(state: &State) -> Result<Snapshot, Error> {
    let selfdrive = state
        .topic("selfdriveState")
        .map_err(|error| Error::Source(error.to_string()))?;
    let carrot = state
        .topic("carrotMan")
        .map_err(|error| Error::Source(error.to_string()))?;
    let car = state
        .topic("carState")
        .map_err(|error| Error::Source(error.to_string()))?;
    let mut result = Snapshot {
        now: clock::now()?,
        received: selfdrive.receive_time,
        valid: selfdrive.valid,
        enabled: false,
        alert: 0,
        updated: selfdrive.updated,
        countdown: 100,
        countdown_valid: carrot.valid,
        countdown_updated: carrot.updated,
        car_valid: car.valid,
        buttons: Vec::new(),
    };
    if selfdrive.valid {
        let event::Which::SelfdriveState(value) = selfdrive
            .event()
            .map_err(|error| Error::Source(error.to_string()))?
            .which()
            .map_err(capnp::Error::from)
            .map_err(cereal)?
        else {
            return Err(Error::Source("expected selfdriveState event".into()));
        };
        let value = value.map_err(cereal)?;
        result.enabled = value.get_enabled();
        result.alert = match value.get_alert_sound() {
            Ok(alert) => u16::from(alert),
            Err(capnp::NotInSchema(raw)) => raw,
        };
    }
    if carrot.valid {
        let event::Which::CarrotMan(value) = carrot
            .event()
            .map_err(|error| Error::Source(error.to_string()))?
            .which()
            .map_err(capnp::Error::from)
            .map_err(cereal)?
        else {
            return Err(Error::Source("expected carrotMan event".into()));
        };
        result.countdown = value.map_err(cereal)?.get_left_sec();
    }
    if car.valid {
        let event::Which::CarState(value) = car
            .event()
            .map_err(|error| Error::Source(error.to_string()))?
            .which()
            .map_err(capnp::Error::from)
            .map_err(cereal)?
        else {
            return Err(Error::Source("expected carState event".into()));
        };
        for button in value.map_err(cereal)?.get_button_events().map_err(cereal)? {
            let kind = match button.get_type() {
                Ok(kind) => u16::from(kind),
                Err(capnp::NotInSchema(raw)) => raw,
            };
            result.buttons.push(Button {
                kind,
                pressed: button.get_pressed(),
            });
        }
    }
    Ok(result)
}
