use crate::Error;
use openpilot_params::{KeyInfo, Params, KEYS};

pub trait Parameters {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, Error>;
    fn put(&self, key: &str, value: &[u8]) -> Result<(), Error>;
    fn clear(&self, flags: u32) -> Result<(), Error>;
    fn cast_failed(&self, info: &KeyInfo, value: &[u8]) -> Result<(), Error> {
        crate::diagnostics::cast_failed(
            &mut openpilot_logging::producer::Factory::for_runtime()?.logger(),
            info,
            value,
        )
    }
    fn boolean(&self, key: &str) -> Result<bool, Error> {
        Ok(self.get(key)?.as_deref() == Some(b"1"))
    }
    fn put_bool(&self, key: &str, value: bool) -> Result<(), Error> {
        self.put(key, if value { b"1" } else { b"0" })
    }
}
impl Parameters for Params {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, Error> {
        match Params::get(self, key) {
            Ok(value) => Ok(value.filter(|value| !value.is_empty())),
            Err(openpilot_params::Error::Io(_)) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
    fn put(&self, key: &str, value: &[u8]) -> Result<(), Error> {
        match Params::put(self, key, value) {
            Ok(()) | Err(openpilot_params::Error::Io(_)) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
    fn clear(&self, flags: u32) -> Result<(), Error> {
        match Params::clear(self, flags) {
            Ok(()) | Err(openpilot_params::Error::Io(_)) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

enum DefaultState<'a> {
    Missing,
    Invalid(&'a [u8]),
    Existing,
}

/// Keep failed conversions distinct from absent values so diagnostics precede the write.
fn default_state<'a>(value: Option<&'a [u8]>, info: &KeyInfo) -> DefaultState<'a> {
    let Some(value) = value.filter(|value| !value.is_empty()) else {
        return DefaultState::Missing;
    };
    let invalid = match info.kind {
        1 | 6 => false,
        0 => std::str::from_utf8(value).is_err(),
        2 => !integer_bytes(value),
        3 => !float_bytes(value),
        5 => match std::str::from_utf8(value)
            .ok()
            .and_then(|text| openpilot_logmessaged::JsonValue::parse(text).ok())
        {
            Some(value) => {
                if matches!(value.view(), openpilot_logmessaged::JsonView::Null) {
                    return DefaultState::Missing;
                }
                false
            }
            None => true,
        },
        _ => false,
    };
    if invalid {
        DefaultState::Invalid(value)
    } else {
        DefaultState::Existing
    }
}

pub fn set_defaults(params: &impl Parameters, overwrite: bool) -> Result<(), Error> {
    for info in KEYS {
        if let Some(value) = info.default {
            let raw = if overwrite {
                None
            } else {
                params.get(info.name)?
            };
            let replace = match default_state(raw.as_deref(), info) {
                DefaultState::Missing => true,
                DefaultState::Invalid(bytes) => {
                    params.cast_failed(info, bytes)?;
                    true
                }
                DefaultState::Existing => false,
            };
            if replace {
                params.put(info.name, value.as_bytes())?;
                if overwrite {
                    println!("SetToDefault[{}]={value}", info.name);
                }
            }
        }
    }
    Ok(())
}

pub fn write_onroad(params: &impl Parameters, started: bool) -> Result<(), Error> {
    params.put_bool("IsOnroad", started)?;
    params.put_bool("IsOffroad", !started)
}

fn integer_bytes(bytes: &[u8]) -> bool {
    let value = bytes.trim_ascii();
    let value = match value.first() {
        Some(b'+' | b'-') => &value[1..],
        _ => value,
    };
    let mut previous_digit = false;
    let mut digits = 0_usize;
    for &byte in value {
        if byte.is_ascii_digit() {
            previous_digit = true;
            digits += 1;
        } else if byte == b'_' && previous_digit {
            previous_digit = false;
        } else {
            return false;
        }
    }
    // Python's default decimal conversion limit is 4300 digits.
    previous_digit && digits <= 4300
}
fn float_bytes(bytes: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(bytes.trim_ascii()) else {
        return false;
    };
    let bytes = text.as_bytes();
    for (index, &byte) in bytes.iter().enumerate() {
        if byte == b'_'
            && !(index > 0
                && bytes[index - 1].is_ascii_digit()
                && bytes.get(index + 1).is_some_and(u8::is_ascii_digit))
        {
            return false;
        }
    }
    text.replace('_', "").parse::<f64>().is_ok()
}
