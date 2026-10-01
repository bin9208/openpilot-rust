use crate::{
    types::{diagonal, Car},
    Error,
};
use capnp::{message::ReaderOptions, serialize};
use openpilot_cereal::log_capnp::event;
use serde_json::value::RawValue;
use std::collections::HashMap;

pub trait Store {
    fn get(&mut self, key: &'static str) -> Option<Vec<u8>>;
    fn put(&mut self, key: &'static str, bytes: Vec<u8>);
    fn remove(&mut self, key: &'static str);
}
impl Store for openpilot_params::Params {
    fn get(&mut self, key: &'static str) -> Option<Vec<u8>> {
        openpilot_params::Params::get(self, key)
            .ok()
            .flatten()
            .filter(|v| !v.is_empty())
    }
    fn put(&mut self, key: &'static str, bytes: Vec<u8>) {
        let _ = openpilot_params::Params::put(self, key, &bytes);
    }
    fn remove(&mut self, key: &'static str) {
        let _ = openpilot_params::Params::remove(self, key);
    }
}

enum Legacy {
    Absent,
    Invalid,
    Values(Result<[f64; 3], ()>),
}
fn legacy(bytes: Option<Vec<u8>>) -> Legacy {
    let Some(bytes) = bytes else {
        return Legacy::Absent;
    };
    let Some(text) = crate::legacy_encoding::decode(&bytes) else {
        return Legacy::Invalid;
    };
    let bytes = text.as_bytes();
    let mut normalized = bytes.to_vec();
    let (mut quoted, mut escaped, mut i) = (false, false, 0);
    while i < bytes.len() {
        let value = bytes[i];
        if quoted {
            if escaped {
                escaped = false;
            } else if value == b'\\' {
                escaped = true;
            } else if value == b'"' {
                quoted = false;
            }
            i += 1;
            continue;
        }
        if value == b'"' {
            quoted = true;
            i += 1;
            continue;
        }
        let replacement = if bytes[i..].starts_with(b"-Infinity") {
            Some((9, b"-1e999999".as_slice()))
        } else if bytes[i..].starts_with(b"Infinity") {
            Some((8, b"1e999999".as_slice()))
        } else if bytes[i..].starts_with(b"NaN") {
            Some((3, b"0.0".as_slice()))
        } else {
            None
        };
        if let Some((length, replacement)) = replacement {
            normalized[i..i + length].copy_from_slice(replacement);
            i += length;
        } else {
            i += 1;
        }
    }
    let Ok(normalized) = std::str::from_utf8(&normalized) else {
        return Legacy::Invalid;
    };
    if serde_json::from_str::<&RawValue>(normalized).is_err() {
        return Legacy::Invalid;
    }
    if normalized.trim() == "null" {
        return Legacy::Absent;
    }
    let Ok(fields) = serde_json::from_str::<HashMap<String, &RawValue>>(normalized) else {
        return Legacy::Values(Err(()));
    };
    let number = |name: &str| -> Result<f64, ()> {
        let field = fields.get(name).ok_or(())?.get();
        let offset = field.as_ptr().addr() - normalized.as_ptr().addr();
        let original = &text[offset..offset + field.len()];
        match original {
            "NaN" => Ok(f64::NAN),
            "Infinity" => Ok(f64::INFINITY),
            "-Infinity" => Ok(f64::NEG_INFINITY),
            _ if original.starts_with(|c: char| c.is_ascii_digit() || c == '-') => {
                let value: f64 = original.parse().map_err(|_| ())?;
                if !original.contains(['.', 'e', 'E']) && !value.is_finite() {
                    return Err(());
                }
                Ok(value)
            }
            _ => Err(()),
        }
    };
    Legacy::Values((|| {
        Ok([
            number("steerRatio")?,
            number("stiffnessFactor")?,
            number("angleOffsetAverageDeg")?,
        ])
    })())
}
pub fn migrate(store: &mut impl Store, time: u64, logs: &mut Vec<(String, String)>) {
    let old = legacy(store.get("LiveParameters"));
    if matches!(old, Legacy::Invalid) {
        logs.push((
            "warning".into(),
            "Failed to cast param LiveParameters from JSON".into(),
        ));
    }
    let current = store.get("LiveParametersV2");
    if current.is_some() {
        return;
    }
    let Legacy::Values(values) = old else {
        return;
    };
    match values {
        Ok([ratio, stiffness, offset]) => {
            let mut message = capnp::message::Builder::new_default();
            let mut root = message.init_root::<event::Builder<'_>>();
            root.set_log_mono_time(time);
            root.set_valid(false);
            let mut parameters = root.init_live_parameters();
            parameters.set_valid(true);
            parameters.set_steer_ratio(ratio as f32);
            parameters.set_stiffness_factor(stiffness as f32);
            parameters.set_angle_offset_average_deg(offset as f32);
            store.put(
                "LiveParametersV2",
                serialize::write_message_to_words(&message),
            );
        }
        Err(()) => {
            logs.push((
                "error".into(),
                "Failed to perform parameter migration: invalid fields".into(),
            ));
            store.remove("LiveParameters");
        }
    }
}
pub struct Initial {
    pub ratio: f64,
    pub stiffness: f64,
    pub offset_degrees: f64,
    pub covariance: Option<Vec<f64>>,
}
fn cached(bytes: &[u8], previous: &[u8], car: &Car, debug: bool) -> Result<Initial, Error> {
    let message = serialize::read_message(bytes, ReaderOptions::new())?;
    let root = message.get_root::<event::Reader<'_>>()?;
    let event::LiveParameters(value) = root.which()? else {
        return Err(Error::Contract("cached service"));
    };
    let value = value?;
    if crate::wire::car(previous)?.fingerprint != car.fingerprint {
        return Err(Error::Contract("Car model mismatch"));
    }
    let ratio = f64::from(value.get_steer_ratio());
    if !(0.5 * car.ratio <= ratio && ratio <= 2. * car.ratio) {
        return Err(Error::Contract("Invalid starting values"));
    }
    let std = value.get_debug_filter_state()?.get_std()?;
    let covariance = if debug && !std.is_empty() {
        Some(diagonal(&std.iter().collect::<Vec<_>>()))
    } else {
        None
    };
    Ok(Initial {
        ratio,
        stiffness: f64::from(value.get_stiffness_factor()),
        offset_degrees: f64::from(value.get_angle_offset_average_deg()),
        covariance,
    })
}
pub fn retrieve(
    store: &mut impl Store,
    car: &Car,
    replay: bool,
    debug: bool,
    logs: &mut Vec<(String, String)>,
) -> Initial {
    let value = store.get("LiveParametersV2");
    let previous = store.get("CarParamsPrevRoute");
    let mut initial = None;
    if let (Some(value), Some(previous)) = (value, previous) {
        match cached(&value, &previous, car, debug) {
            Ok(value) => initial = Some(value),
            Err(error) => {
                logs.push((
                    "error".into(),
                    format!("Failed to retrieve initial values: {error}"),
                ));
                store.remove("LiveParametersV2");
            }
        }
    }
    let mut initial = initial.unwrap_or_else(|| {
        logs.push((
            "info".into(),
            "Parameter learner resetting to default values".into(),
        ));
        Initial {
            ratio: car.ratio,
            stiffness: 1.,
            offset_degrees: 0.,
            covariance: None,
        }
    });
    if !replay {
        initial.stiffness = 1.;
    }
    initial
}
