//! Nonblocking STRING Params conversion and its source-compatible diagnostic boundary.
use openpilot_logging::{
    log_site,
    producer::Logger,
    record::{Level, Record},
};
use openpilot_params::{metadata, Params};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error("Params key {key} has type {kind}, expected STRING")]
    NotString { key: String, kind: u8 },
    #[error(transparent)]
    Logging(#[from] openpilot_logging::Error),
}

pub fn get_string(
    params: &Params,
    key: &str,
    logger: &mut Logger,
) -> Result<Option<String>, Error> {
    let info = metadata(key).ok_or_else(|| openpilot_params::Error::UnknownKey(key.to_owned()))?;
    if info.kind != 0 {
        return Err(Error::NotString {
            key: key.to_owned(),
            kind: info.kind,
        });
    }
    let bytes = match params.get(key) {
        Ok(Some(bytes)) if !bytes.is_empty() => bytes,
        Ok(_) | Err(openpilot_params::Error::Io(_)) => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    match String::from_utf8(bytes) {
        Ok(value) => Ok(Some(value)),
        Err(error) => {
            logger.emit(
                log_site!(),
                Record::text(
                    Level::Warning,
                    format!(
                "Failed to cast param {key} with value={} from type t=<ParamKeyType.STRING: 0>",
                bytes_repr(error.as_bytes()),
            ),
                ),
            )?;
            Ok(None)
        }
    }
}

fn bytes_repr(bytes: &[u8]) -> String {
    let quote = if bytes.contains(&b'\'') && !bytes.contains(&b'"') {
        b'"'
    } else {
        b'\''
    };
    let mut output = String::from("b");
    output.push(char::from(quote));
    for &byte in bytes {
        match byte {
            b'\t' => output.push_str("\\t"),
            b'\n' => output.push_str("\\n"),
            b'\r' => output.push_str("\\r"),
            b'\\' => output.push_str("\\\\"),
            value if value == quote => {
                output.push('\\');
                output.push(char::from(value));
            }
            32..=126 => output.push(char::from(byte)),
            value => {
                const HEX: &[u8; 16] = b"0123456789abcdef";
                output.push_str("\\x");
                output.push(char::from(HEX[usize::from(value >> 4)]));
                output.push(char::from(HEX[usize::from(value & 15)]));
            }
        }
    }
    output.push(char::from(quote));
    output
}
