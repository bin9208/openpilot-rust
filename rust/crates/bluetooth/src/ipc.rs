use crate::{runtime::RuntimeError, VehicleSnapshot};
use openpilot_cereal::{car_capnp::car_state::GearShifter, log_capnp::event};
use openpilot_messaging::runtime::SubMaster;

pub(crate) fn snapshot(subscriber: &SubMaster) -> Result<VehicleSnapshot, RuntimeError> {
    let device_topic = subscriber.state.topic("deviceState")?;
    let car_topic = subscriber.state.topic("carState")?;
    let controls_topic = subscriber.state.topic("selfdriveState")?;
    let device = match device_topic.event()?.which()? {
        event::DeviceState(value) => value?,
        _ => return Err(RuntimeError::Contract("deviceState event required")),
    };
    let car = match car_topic.event()?.which()? {
        event::CarState(value) => value?,
        _ => return Err(RuntimeError::Contract("carState event required")),
    };
    let controls = match controls_topic.event()?.which()? {
        event::SelfdriveState(value) => value?,
        _ => return Err(RuntimeError::Contract("selfdriveState event required")),
    };
    Ok(VehicleSnapshot {
        device_alive: device_topic.alive,
        started: device.get_started(),
        car_alive: car_topic.alive,
        car_valid: car_topic.valid,
        can_valid: car.get_can_valid(),
        controls_alive: controls_topic.alive,
        enabled: controls.get_enabled(),
        brake: car.get_brake_pressed(),
        gas: car.get_gas_pressed(),
        gear_drive: matches!(car.get_gear_shifter(), Ok(GearShifter::Drive)),
        physical_buttons: !car.get_button_events()?.is_empty(),
        v_ego: f64::from(car.get_v_ego()),
    })
}
