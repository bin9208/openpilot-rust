use crate::Error;
use openpilot_logging::{
    log_site,
    producer::Logger,
    record::{Level, Record},
    Fields, Value,
};
use openpilot_params::KeyInfo;
use openpilot_runtime_version::JsonValue;
use std::{ffi::OsStr, os::unix::ffi::OsStrExt};

pub fn start(logger: &mut Logger) -> Result<(), Error> {
    logger.bind(
        [("daemon".into(), Value::Text("manager".into()))]
            .into_iter()
            .collect(),
    );
    logger.emit(
        log_site!(),
        Record::text(Level::Info, "manager start".into()),
    )?;
    // SwagFormatter's fallback serializes os.environ using repr, not as a dict.
    let environment = environment_repr()?;
    let fields: Fields = [("environ".into(), Value::Text(environment))]
        .into_iter()
        .collect();
    let mut record = Record::text(
        Level::Info,
        fields.to_json().map_err(openpilot_logging::Error::from)?,
    );
    record.message = Value::Object(fields);
    logger.emit(log_site!(), record)?;
    Ok(())
}
pub fn cleanup_finished(logger: &mut Logger) -> Result<(), Error> {
    logger.emit(
        log_site!(),
        Record::text(Level::Info, "everything is dead".into()),
    )?;
    Ok(())
}
pub fn cast_failed(logger: &mut Logger, info: &KeyInfo, value: &[u8]) -> Result<(), Error> {
    let kind = match info.kind {
        0 => "STRING",
        1 => "BOOL",
        2 => "INT",
        3 => "FLOAT",
        4 => "TIME",
        5 => "JSON",
        6 => "BYTES",
        _ => return Err(Error::Contract("unknown Params kind")),
    };
    logger.emit(
        log_site!(),
        Record::text(
            Level::Warning,
            format!(
                "Failed to cast param {} with value={} from type t=<ParamKeyType.{kind}: {}>",
                bytes_repr(info.name.as_bytes()),
                bytes_repr(value),
                info.kind,
            ),
        ),
    )?;
    Ok(())
}
fn environment_repr() -> Result<String, Error> {
    let mut json = String::from("{");
    for (index, (key, value)) in std::env::vars_os().enumerate() {
        if index > 0 {
            json.push(',');
        }
        json.push_str(
            &os_text(&key)?
                .to_json()
                .map_err(openpilot_runtime_version::Error::from)?,
        );
        json.push(':');
        json.push_str(
            &os_text(&value)?
                .to_json()
                .map_err(openpilot_runtime_version::Error::from)?,
        );
    }
    json.push('}');
    let value = JsonValue::parse(&json).map_err(openpilot_runtime_version::Error::from)?;
    let text: String = openpilot_runtime_version::python_str(&value)?
        .into_iter()
        .map(char::from_u32)
        .collect::<Option<_>>()
        .ok_or(Error::Contract("invalid environment repr"))?;
    Ok(format!("environ({text})"))
}
// Unix os.environ decodes bytes with UTF-8 surrogateescape.
fn os_text(value: &OsStr) -> Result<JsonValue, Error> {
    let mut bytes = value.as_bytes();
    let mut points = Vec::new();
    while !bytes.is_empty() {
        match std::str::from_utf8(bytes) {
            Ok(text) => {
                points.extend(text.chars().map(u32::from));
                break;
            }
            Err(error) => {
                let valid = error.valid_up_to();
                let prefix = std::str::from_utf8(&bytes[..valid])
                    .map_err(|_| Error::Contract("invalid UTF-8 prefix"))?;
                points.extend(prefix.chars().map(u32::from));
                points.push(0xdc00 + u32::from(bytes[valid]));
                bytes = &bytes[valid + 1..];
            }
        }
    }
    JsonValue::codepoints(points).ok_or(Error::Contract("invalid environment codepoint"))
}
fn bytes_repr(bytes: &[u8]) -> String {
    let quote = if bytes.contains(&b'\'') && !bytes.contains(&b'"') {
        b'"'
    } else {
        b'\''
    };
    let mut text = String::from("b");
    text.push(char::from(quote));
    for &byte in bytes {
        match byte {
            b'\t' => text.push_str("\\t"),
            b'\n' => text.push_str("\\n"),
            b'\r' => text.push_str("\\r"),
            b'\\' => text.push_str("\\\\"),
            value if value == quote => {
                text.push('\\');
                text.push(char::from(value));
            }
            32..=126 => text.push(char::from(byte)),
            value => text.push_str(&format!("\\x{value:02x}")),
        }
    }
    text.push(char::from(quote));
    text
}
