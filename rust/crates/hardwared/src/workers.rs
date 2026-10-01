use crate::{
    host::{hardware_json, Config},
    Error,
};
use openpilot_logging::{
    log_site,
    producer::{Factory, Logger},
    record::{Level, Record},
    Value as LogValue,
};
use openpilot_messaging::runtime::PubMaster;
use serde_json::{json, Value};
use std::{
    fs::OpenOptions,
    io::Read,
    os::unix::fs::OpenOptionsExt,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{SyncSender, TrySendError},
    },
    time::Duration,
};

pub fn log_event(logger: &mut Logger, name: &str, fields: Value) -> Result<(), Error> {
    let LogValue::Object(fields) = LogValue::from_json(fields)? else {
        return Err(Error::Contract("event fields must be object"));
    };
    logger.emit(log_site!(), Record::event(name, vec![], fields)?)?;
    Ok(())
}
pub fn log_error(
    logger: &mut Logger,
    name: &str,
    error: &dyn std::fmt::Display,
) -> Result<(), Error> {
    logger.emit(
        log_site!(),
        Record::text(Level::Error, name.into()).with_exception(error.to_string()),
    )?;
    Ok(())
}
pub fn sleep(stop: &AtomicBool, duration: Duration) {
    let end = std::time::Instant::now() + duration;
    while !stop.load(Ordering::Relaxed) {
        let Some(remaining) = end.checked_duration_since(std::time::Instant::now()) else {
            break;
        };
        std::thread::sleep(remaining.min(Duration::from_millis(50)));
    }
}
pub fn network(
    config: &Config,
    sender: SyncSender<Value>,
    stop: &AtomicBool,
    factory: &Factory,
) -> Result<(), Error> {
    let hardware = config.hardware();
    let mut logger = factory.logger();
    let mut previous_temperatures = json!([]);
    let mut modem_version = Value::Null;
    let mut count = 0_u64;
    while !stop.load(Ordering::Relaxed) {
        if count.is_multiple_of(20) {
            let result = (|| -> Result<Value, Error> {
                let network_type = hardware.get_network_type()?;
                let mut temperatures = hardware_json(hardware.get_modem_temperatures()?)?;
                if temperatures.as_array().is_some_and(Vec::is_empty) {
                    temperatures = previous_temperatures.clone();
                }
                if config.agnos && modem_version.is_null() {
                    modem_version = hardware_json(hardware.get_modem_version()?)?;
                    if !modem_version.is_null() {
                        log_event(
                            &mut logger,
                            "modem version",
                            json!({"version":modem_version}),
                        )?;
                    }
                }
                let (tx, rx) = hardware.get_modem_data_usage()?;
                let info = hardware.get_network_info()?;
                let strength = hardware.get_network_strength(network_type)?;
                let metered = hardware.get_network_metered(network_type)?;
                let mut value = json!({
                    "networkType":network_type.0, "networkStrength":strength.ordinal(),
                    "networkMetered":metered, "modemTempC":temperatures,
                    "networkStats":{"wwanTx":hardware_json(tx)?,"wwanRx":hardware_json(rx)?},
                });
                if let Some(info) = info {
                    value["networkInfo"] = hardware_json(info)?;
                }
                Ok(value)
            })();
            match result {
                Ok(value) => {
                    previous_temperatures = value["modemTempC"].clone();
                    match sender.try_send(value) {
                        Ok(()) | Err(TrySendError::Full(_)) => {}
                        Err(TrySendError::Disconnected(_)) => break,
                    }
                }
                Err(error) => log_error(&mut logger, "Error getting hardware state", &error)?,
            }
        }
        count += 1;
        sleep(stop, Duration::from_millis(500));
    }
    Ok(())
}
/// Linux input_event is two native longs, two u16 fields and an i32 value.
/// Supported runtime targets (x86_64/aarch64 Linux) both use 64-bit longs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Touch {
    pub sec: i64,
    pub usec: i64,
    pub kind: u16,
    pub code: u16,
    pub value: i32,
}
impl Touch {
    pub fn decode(bytes: &[u8; 24]) -> Self {
        Self {
            sec: i64::from_ne_bytes(bytes[0..8].try_into().unwrap_or([0; 8])),
            usec: i64::from_ne_bytes(bytes[8..16].try_into().unwrap_or([0; 8])),
            kind: u16::from_ne_bytes([bytes[16], bytes[17]]),
            code: u16::from_ne_bytes([bytes[18], bytes[19]]),
            value: i32::from_ne_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]),
        }
    }
}
pub fn touch(config: &Config, stop: &AtomicBool) -> Result<(), Error> {
    let mut publisher = PubMaster::for_runtime(&["touch"])?;
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32)
        .open(
            config
                .root
                .join("dev/input/by-path/platform-894000.i2c-event"),
        )?;
    let mut frame = Vec::new();
    let mut count = 0_u64;
    while !stop.load(Ordering::Relaxed) {
        if count.is_multiple_of(2) {
            let mut bytes = [0; 24];
            let length = match file.read(&mut bytes) {
                Ok(length) => length,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => 0,
                Err(error) => return Err(error.into()),
            };
            if length != 0 {
                if length != bytes.len() {
                    return Err(Error::Contract("partial Linux input_event"));
                }
                let event = Touch::decode(&bytes);
                if event.kind != 0 || event.code != 0 || event.value != 0 {
                    frame.push(event);
                } else {
                    let mut message = capnp::message::Builder::new_default();
                    let mut root =
                        message.init_root::<openpilot_cereal::log_capnp::event::Builder>();
                    root.set_valid(true);
                    root.set_log_mono_time((crate::monotonic() * 1e9) as u64);
                    let mut list = root.init_touch(
                        u32::try_from(frame.len())
                            .map_err(|_| Error::Contract("touch frame too long"))?,
                    );
                    for (index, value) in frame.drain(..).enumerate() {
                        let mut item = list.reborrow().get(index as u32);
                        item.set_sec(value.sec);
                        item.set_usec(value.usec);
                        item.set_type(
                            u8::try_from(value.kind)
                                .map_err(|_| Error::Contract("touch type exceeds UInt8"))?,
                        );
                        item.set_code(i32::from(value.code));
                        item.set_value(value.value);
                    }
                    publisher.send("touch", &capnp::serialize::write_message_to_words(&message))?;
                }
                continue;
            }
        }
        count += 1;
        sleep(stop, Duration::from_millis(500));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Touch;
    #[test]
    fn input_event_preserves_signed_values_and_native_field_widths() {
        // Given one native Linux input_event from an owned fixture.
        let mut bytes = [0; 24];
        bytes[..8].copy_from_slice(&123_i64.to_ne_bytes());
        bytes[8..16].copy_from_slice(&456_i64.to_ne_bytes());
        bytes[16..18].copy_from_slice(&3_u16.to_ne_bytes());
        bytes[18..20].copy_from_slice(&65535_u16.to_ne_bytes());
        bytes[20..].copy_from_slice(&(-17_i32).to_ne_bytes());
        // When decoding the kernel ABI record without a pointer cast.
        let actual = Touch::decode(&bytes);
        // Then the signed payload and full input code survive decoding.
        assert_eq!(
            actual,
            Touch {
                sec: 123,
                usec: 456,
                kind: 3,
                code: 65535,
                value: -17
            }
        );
    }
}
