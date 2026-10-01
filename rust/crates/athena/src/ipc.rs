use crate::{state::Stop, Error};
use capnp::{dynamic_struct, dynamic_value, message::ReaderOptions, serialize};
use openpilot_cereal::log_capnp::event;
use openpilot_logmessaged::JsonValue;
use std::{
    io::Cursor,
    time::{Duration, Instant},
};

pub fn message(service: &str, timeout: i64, stop: &Stop) -> Result<JsonValue, Error> {
    let service =
        openpilot_messaging::services::lookup(service).ok_or(Error::Contract("invalid service"))?;
    let mut socket =
        openpilot_msgq::Subscriber::for_runtime(service.name, false, service.queue_size)?;
    let started = Instant::now();
    loop {
        if stop.requested() {
            return Err(Error::Stopped);
        }
        let wait = if timeout < 0 {
            Duration::from_millis(100)
        } else {
            Duration::from_millis(
                u64::try_from(timeout).map_err(|_| Error::Contract("timeout range"))?,
            )
            .saturating_sub(started.elapsed())
            .min(Duration::from_millis(100))
        };
        if let Some(bytes) = socket.receive(wait)? {
            return decode(&bytes);
        }
        if timeout >= 0
            && started.elapsed().as_millis()
                >= u128::try_from(timeout).map_err(|_| Error::Contract("timeout range"))?
        {
            return Err(Error::Timeout);
        }
    }
}
pub fn decode(bytes: &[u8]) -> Result<JsonValue, Error> {
    let reader = serialize::read_message(&mut Cursor::new(bytes), ReaderOptions::new())?;
    let event = reader.get_root::<event::Reader<'_>>()?;
    let dynamic_value::Reader::Struct(event) = event.into() else {
        return Err(Error::Contract("Event must be a struct"));
    };
    JsonValue::parse(&structure(event)?).map_err(Error::from)
}
pub fn device(
    subscriber: &openpilot_messaging::runtime::SubMaster,
) -> Result<openpilot_cereal::log_capnp::device_state::Reader<'_>, Error> {
    match subscriber.state.topic("deviceState")?.event()?.which()? {
        event::DeviceState(device) => Ok(device?),
        _ => Err(Error::Contract("deviceState event required")),
    }
}
fn structure(value: dynamic_struct::Reader<'_>) -> Result<String, Error> {
    let mut fields = Vec::new();
    let active = value.which()?;
    for field in value.get_schema().get_fields()?.iter() {
        if active == Some(field) || value.has(field)? {
            let name = field.get_proto().get_name()?.to_str()?;
            fields.push(format!(
                "{}:{}",
                serde_json::to_string(name)?,
                dynamic(value.get(field)?)?
            ));
        }
    }
    Ok(format!("{{{}}}", fields.join(",")))
}
fn dynamic(value: dynamic_value::Reader<'_>) -> Result<String, Error> {
    use dynamic_value::Reader as V;
    Ok(match value {
        V::Void => "null".into(),
        V::Bool(value) => value.to_string(),
        V::Int8(value) => value.to_string(),
        V::Int16(value) => value.to_string(),
        V::Int32(value) => value.to_string(),
        V::Int64(value) => value.to_string(),
        V::UInt8(value) => value.to_string(),
        V::UInt16(value) => value.to_string(),
        V::UInt32(value) => value.to_string(),
        V::UInt64(value) => value.to_string(),
        V::Float32(value) => float(f64::from(value)),
        V::Float64(value) => float(value),
        V::Text(value) => serde_json::to_string(value.to_str()?)?,
        V::Enum(value) => match value.get_enumerant()? {
            Some(item) => serde_json::to_string(item.get_proto().get_name()?.to_str()?)?,
            None => return Err(Error::Contract("unknown cereal enum")),
        },
        V::Struct(value) => structure(value)?,
        V::List(value) => format!(
            "[{}]",
            value
                .iter()
                .map(|item| dynamic(item?))
                .collect::<Result<Vec<_>, Error>>()?
                .join(",")
        ),
        V::Data(_) => {
            return Err(Error::Contract(
                "Object of type bytes is not JSON serializable",
            ))
        }
        V::AnyPointer(_) | V::Capability(_) => {
            return Err(Error::Contract("cereal pointer is not JSON serializable"))
        }
    })
}
fn float(value: f64) -> String {
    if value.is_nan() {
        "NaN".into()
    } else if value == f64::INFINITY {
        "Infinity".into()
    } else if value == f64::NEG_INFINITY {
        "-Infinity".into()
    } else {
        value.to_string()
    }
}
