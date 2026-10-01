use openpilot_cereal::log_capnp::{
    event,
    sensor_event_data::{self, SensorSource},
};
use openpilot_sensord::Error;
use serde_json::{json, Value};
fn read_sensor(sensor: sensor_event_data::Reader<'_>) -> Result<Value, Error> {
    let source = match sensor
        .get_source()
        .map_err(|_| Error::Contract("unknown sensor source"))?
    {
        SensorSource::Velodyne => "velodyne",
        SensorSource::Lsm6ds3 => "lsm6ds3",
        SensorSource::Lsm6ds3trc => "lsm6ds3trc",
        _ => return Err(Error::Contract("unexpected sensor source")),
    };
    let deprecated = sensor.get_deprecated();
    let mut result = json!({"timestamp":sensor.get_timestamp(),"source":source,"deprecated":{"version":deprecated.get_version(),"sensor":deprecated.get_sensor(),"type":deprecated.get_type(),"uncalibrated":deprecated.get_uncalibrated()}});
    match sensor
        .which()
        .map_err(|_| Error::Contract("unknown sensor value"))?
    {
        sensor_event_data::Which::Acceleration(v) => {
            let v = v?;
            result["acceleration"] = json!({"v":v.get_v()?.iter().map(f64::from).collect::<Vec<_>>(),"deprecated":{"status":v.get_deprecated().get_status()}});
        }
        sensor_event_data::Which::GyroUncalibrated(v) => {
            let v = v?;
            result["gyroUncalibrated"] = json!({"v":v.get_v()?.iter().map(f64::from).collect::<Vec<_>>(),"deprecated":{"status":v.get_deprecated().get_status()}});
        }
        sensor_event_data::Which::Temperature(v) => result["temperature"] = json!(f64::from(v)),
        _ => return Err(Error::Contract("unexpected sensor value")),
    }
    Ok(result)
}
pub fn packet(bytes: &[u8]) -> Result<Value, Error> {
    let message = capnp::serialize::read_message(bytes, capnp::message::ReaderOptions::new())?;
    let root = message.get_root::<event::Reader<'_>>()?;
    let (service, sensor) = match root.which().map_err(|_| Error::Contract("unknown event"))? {
        event::Which::Accelerometer(v) => ("accelerometer", v?),
        event::Which::Gyroscope(v) => ("gyroscope", v?),
        event::Which::TemperatureSensor(v) => ("temperatureSensor", v?),
        _ => return Err(Error::Contract("unexpected event")),
    };
    Ok(
        json!({"service":service,"valid":root.get_valid(),"logMonoTime":root.get_log_mono_time(),"event":read_sensor(sensor)?}),
    )
}
