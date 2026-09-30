use crate::{
    sensor::{Event, Kind, Source, Value},
    Error,
};
use openpilot_cereal::log_capnp::{
    event,
    sensor_event_data::{self, SensorSource},
};
pub fn fill(mut builder: sensor_event_data::Builder<'_>, event: &Event) {
    builder.set_timestamp(event.timestamp);
    builder.set_source(match event.source {
        Source::Velodyne => SensorSource::Velodyne,
        Source::Lsm6ds3 => SensorSource::Lsm6ds3,
        Source::Lsm6ds3trc => SensorSource::Lsm6ds3trc,
    });
    match &event.value {
        Value::Acceleration(values) => {
            let mut v = builder.init_acceleration().init_v(3);
            for (i, value) in (0..3).zip(values) {
                v.set(i, *value);
            }
        }
        Value::GyroUncalibrated(values) => {
            let mut v = builder.init_gyro_uncalibrated().init_v(3);
            for (i, value) in (0..3).zip(values) {
                v.set(i, *value);
            }
        }
        Value::Temperature(value) => builder.set_temperature(*value),
    }
}
pub fn encode(kind: Kind, value: &Event, log_time: u64) -> Result<Vec<u8>, Error> {
    let mut message = capnp::message::Builder::new_default();
    let mut root = message.init_root::<event::Builder<'_>>();
    root.set_valid(true);
    root.set_log_mono_time(log_time);
    let event = match kind {
        Kind::Accelerometer => root.init_accelerometer(),
        Kind::Gyroscope => root.init_gyroscope(),
        Kind::TemperatureSensor => root.init_temperature_sensor(),
    };
    fill(event, value);
    Ok(capnp::serialize::write_message_to_words(&message))
}
