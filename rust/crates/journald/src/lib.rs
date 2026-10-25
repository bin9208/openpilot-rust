#![forbid(unsafe_code)]
pub mod child;
pub mod decode;
mod integer;
mod value;

use openpilot_cereal::log_capnp::event;
use value::Value;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Json(#[from] decode::Error),
    #[error("journalctl JSON root must be an object")]
    Root,
    #[error("invalid journalctl field {0}")]
    Field(&'static str),
    #[error(transparent)]
    Format(#[from] std::fmt::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug)]
pub struct Entry {
    pub timestamp: u64,
    pub pid: i32,
    pub priority: u8,
    pub tag: Option<String>,
    pub message: String,
}

pub fn parse_line(line: &str) -> Result<Option<Entry>, Error> {
    let line = line.trim_matches(integer::whitespace);
    if line.is_empty() {
        return Ok(None);
    }
    let mut value = decode::decode(line)?;
    let Value::Object(fields) = &value else {
        return Err(Error::Root);
    };
    let field = |name: &'static str| -> Result<i128, Error> {
        fields.get(name.as_bytes()).map_or(Ok(0), |value| {
            integer::convert(value).ok_or(Error::Field(name))
        })
    };
    let timestamp = u64::try_from(field("__REALTIME_TIMESTAMP")?)
        .map_err(|_| Error::Field("__REALTIME_TIMESTAMP"))?;
    let message = value.to_json()?;
    let pid = i32::try_from(field("_PID")?).map_err(|_| Error::Field("_PID"))?;
    let priority = u8::try_from(field("PRIORITY")?).map_err(|_| Error::Field("PRIORITY"))?;
    // Message serialization and all integer conversions are complete. Retain
    // their original failure order, then transfer the now-unused decoded tag.
    let Value::Object(fields) = &mut value else {
        unreachable!("root already validated")
    };
    let tag = match fields.get_mut(b"SYSLOG_IDENTIFIER".as_slice()) {
        Some(Value::Text(bytes)) => take_tag(Some(bytes))?,
        None => None,
        Some(
            Value::Null
            | Value::Bool(_)
            | Value::Integer(_)
            | Value::Float(_)
            | Value::Array(_)
            | Value::Object(_),
        ) => return Err(Error::Field("SYSLOG_IDENTIFIER")),
    };
    Ok(Some(Entry {
        timestamp,
        pid,
        priority,
        tag,
        message,
    }))
}

fn take_tag(bytes: Option<&mut Vec<u8>>) -> Result<Option<String>, Error> {
    bytes
        .map(|bytes| {
            String::from_utf8(std::mem::take(bytes)).map_err(|_| Error::Field("SYSLOG_IDENTIFIER"))
        })
        .transpose()
}

pub fn packet(entry: &Entry, monotonic_ns: u64) -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    let mut event = message.init_root::<event::Builder>();
    // messaging.new_message() defaults to false; journald never overrides it.
    event.set_valid(false);
    event.set_log_mono_time(monotonic_ns);
    let mut log = event.init_android_log();
    log.set_ts(entry.timestamp);
    log.set_pid(entry.pid);
    log.set_priority(entry.priority);
    if let Some(tag) = &entry.tag {
        log.set_tag(tag.as_str());
    }
    log.set_message(entry.message.as_str());
    capnp::serialize::write_message_to_words(&message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publication_moves_decoded_tag_allocation_after_message_serialization() {
        let mut value =
            decode::decode(r#"{"SYSLOG_IDENTIFIER":"한글kernel","MESSAGE":"payload"}"#).unwrap();
        let Value::Object(fields) = &mut value else {
            panic!("object");
        };
        let Some(Value::Text(bytes)) = fields.get_mut(b"SYSLOG_IDENTIFIER".as_slice()) else {
            panic!("text");
        };
        let pointer = bytes.as_ptr();
        let capacity = bytes.capacity();
        let tag = take_tag(Some(bytes)).unwrap().unwrap();
        assert_eq!(tag, "한글kernel");
        assert_eq!(tag.as_ptr(), pointer);
        assert_eq!(tag.capacity(), capacity);
        assert!(bytes.is_empty());
    }
}
