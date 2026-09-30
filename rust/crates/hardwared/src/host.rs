use crate::Error;
use openpilot_hardware_info::{HardwareInfo, HardwarePaths, Pc, Tici};

use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub struct Config {
    pub root: PathBuf,
    pub board: bool,
    pub agnos: bool,
    pub launcher: PathBuf,
    pub cycles: Option<u64>,
}
impl Config {
    pub fn hardware(&self) -> Box<dyn HardwareInfo> {
        if self.board {
            let mut hardware = Tici::with_paths(HardwarePaths::under(&self.root));
            if self.root != Path::new("/") {
                hardware.params_root = Some(self.root.join("data/params"));
            }
            Box::new(hardware)
        } else {
            Box::new(Pc)
        }
    }
    pub fn params(&self) -> Result<Params, Error> {
        Ok(if self.root == Path::new("/") {
            Params(openpilot_params::Params::for_runtime()?)
        } else {
            Params(openpilot_params::Params::open(
                &self.root.join("data/params"),
                "d",
            )?)
        })
    }
}
/// The Python binding intentionally discards C++ write/remove error codes and
/// util::read_file returns empty on read errors. Preserve that caller boundary.
pub struct Params(pub openpilot_params::Params);
impl Params {
    pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>, Error> {
        match self.0.get(key) {
            Ok(value) => Ok(value.filter(|v| !v.is_empty())),
            Err(openpilot_params::Error::Io(_)) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
    pub fn get_bool(&self, key: &str) -> Result<bool, Error> {
        Ok(self.get(key)?.as_deref() == Some(b"1"))
    }
    pub fn put_bool(&self, key: &str, value: bool) -> Result<(), Error> {
        self.put(key, if value { b"1" } else { b"0" })
    }
    pub fn put(&self, key: &str, bytes: &[u8]) -> Result<(), Error> {
        Self::write_result(self.0.put(key, bytes))
    }
    pub fn remove(&self, key: &str) -> Result<(), Error> {
        Self::write_result(self.0.remove(key))
    }
    fn write_result(result: Result<(), openpilot_params::Error>) -> Result<(), Error> {
        match result {
            Ok(()) | Err(openpilot_params::Error::Io(_)) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
    fn json_present(&self, key: &str) -> Result<bool, Error> {
        Ok(self
            .get(key)?
            .is_some_and(|v| serde_json::from_slice::<Value>(&v).is_ok_and(|v| !v.is_null())))
    }
}
pub fn integer(params: &Params, key: &str) -> Result<i32, Error> {
    let Some(bytes) = params.get(key)? else {
        return Ok(0);
    };
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| Error::Contract("invalid integer Param"))?
        .trim_start_matches([' ', '\t', '\n', '\r', '\x0b', '\x0c']);
    let end = text
        .char_indices()
        .find(|(index, c)| !c.is_ascii_digit() && !(*index == 0 && (*c == '+' || *c == '-')))
        .map_or(text.len(), |(index, _)| index);
    text[..end]
        .parse()
        .map_err(|_| Error::Contract("invalid integer Param"))
}
pub fn hardware_json(value: openpilot_hardware_info::JsonValue) -> Result<Value, Error> {
    Ok(serde_json::from_str(
        &value
            .to_json()
            .map_err(openpilot_hardware_info::Error::from)?,
    )?)
}
fn decimal_integer(bytes: &[u8]) -> Result<Option<i128>, Error> {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Ok(None);
    };
    let text = text.trim();
    let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
    if digits.is_empty() {
        return Ok(None);
    }
    let mut previous_digit = false;
    for byte in digits.bytes() {
        if byte.is_ascii_digit() {
            previous_digit = true;
        } else if byte == b'_' && previous_digit {
            previous_digit = false;
        } else {
            return Ok(None);
        }
    }
    if !previous_digit {
        return Ok(None);
    }
    text.replace('_', "")
        .parse()
        .map(Some)
        .map_err(|_| Error::Contract("integer Param out of supported cereal range"))
}
pub fn numeric(params: &Params, key: &str, default: bool) -> Result<f64, Error> {
    let metadata =
        openpilot_params::metadata(key).ok_or(Error::Contract("unknown numeric Param"))?;
    let fallback = if default {
        metadata.default.and_then(|v| v.parse().ok()).unwrap_or(0.)
    } else {
        0.
    };
    let Some(bytes) = params.get(key)? else {
        return Ok(fallback);
    };
    if metadata.kind == 2 {
        return Ok(decimal_integer(&bytes)?.map_or(fallback, |v| v as f64));
    }
    Ok(std::str::from_utf8(&bytes)
        .ok()
        .and_then(|text| text.trim().parse::<f64>().ok())
        .unwrap_or(fallback))
}
pub fn last_ping(params: &Params) -> Result<Option<Value>, Error> {
    let Some(bytes) = params.get("LastAthenaPingTime")? else {
        return Ok(None);
    };
    decimal_integer(&bytes)?
        .map(serde_json::to_value)
        .transpose()
        .map_err(Error::from)
}
pub fn startup(
    params: &Params,
    free_space: f64,
    pc: bool,
) -> Result<BTreeMap<String, bool>, Error> {
    let mut map = BTreeMap::from([
        (
            "up_to_date".into(),
            !params.json_present("Offroad_ConnectivityNeeded")?
                || params.get_bool("DisableUpdates")?
                || params.get_bool("SnoozeUpdate")?,
        ),
        (
            "no_excessive_actuation".into(),
            !params.json_present("Offroad_ExcessiveActuation")?,
        ),
        ("not_uninstalling".into(), !params.get_bool("DoUninstall")?),
        (
            "accepted_terms".into(),
            params.get("HasAcceptedTerms")?.as_deref()
                == Some(openpilot_runtime_version::TERMS_VERSION.as_bytes()),
        ),
        ("free_space".into(), free_space > 2.),
        (
            "completed_training".into(),
            params.get("CompletedTrainingVersion")?.as_deref()
                == Some(openpilot_runtime_version::TRAINING_VERSION.as_bytes()),
        ),
        (
            "not_driver_view".into(),
            !params.get_bool("IsDriverViewEnabled")?,
        ),
        (
            "not_taking_snapshot".into(),
            !params.get_bool("IsTakingSnapshot")?,
        ),
    ]);
    if !pc {
        map.insert("registered_device".into(), true);
    }
    Ok(map)
}
#[derive(Default)]
pub struct CarCache {
    last_check: f64,
    bytes: Option<Vec<u8>>,
    pub tesla: bool,
}
impl CarCache {
    pub fn update(&mut self, params: &Params, now: f64) -> Result<(), Error> {
        if now - self.last_check < 5. {
            return Ok(());
        }
        self.last_check = now;
        let bytes = params.get("CarParams")?;
        if bytes.as_ref().is_none_or(Vec::is_empty) || bytes == self.bytes {
            return Ok(());
        }
        self.bytes = bytes;
        self.tesla = match &self.bytes {
            Some(bytes) => {
                let result = (|| -> Result<bool, capnp::Error> {
                    let message = capnp::serialize::read_message(
                        &mut bytes.as_slice(),
                        capnp::message::ReaderOptions::new(),
                    )?;
                    Ok(message
                        .get_root::<openpilot_cereal::car_capnp::car_params::Reader>()?
                        .get_brand()?
                        == "tesla")
                })();
                result.unwrap_or(false)
            }
            None => false,
        };
        Ok(())
    }
}
pub struct Usage {
    previous: Vec<(f64, f64)>,
    root: PathBuf,
}
impl Usage {
    pub fn new(root: &Path) -> Result<Self, Error> {
        let mut value = Self {
            previous: Vec::new(),
            root: root.into(),
        };
        value.cpu()?;
        Ok(value)
    }
    pub fn cpu(&mut self) -> Result<Vec<f64>, Error> {
        let text = std::fs::read_to_string(self.root.join("proc/stat"))?;
        let mut current = Vec::new();
        for line in text.lines().filter(|line| {
            line.starts_with("cpu") && line.as_bytes().get(3).is_some_and(u8::is_ascii_digit)
        }) {
            let numbers: Vec<f64> = line
                .split_whitespace()
                .skip(1)
                .map(|v| {
                    v.parse::<f64>()
                        .map_err(|_| Error::Contract("invalid proc/stat"))
                })
                .collect::<Result<_, _>>()?;
            let total: f64 = numbers.iter().take(8).sum();
            let idle =
                numbers.get(3).copied().unwrap_or(0.) + numbers.get(4).copied().unwrap_or(0.);
            current.push((total, idle));
        }
        let result = current
            .iter()
            .enumerate()
            .map(|(index, (total, idle))| {
                let previous = self.previous.get(index).copied().unwrap_or((*total, *idle));
                let delta = (total - previous.0).max(0.);
                let busy = (total - previous.0 - (idle - previous.1)).max(0.);
                if delta == 0. {
                    0.
                } else {
                    (busy / delta * 1000.).round_ties_even() / 10.
                }
            })
            .collect();
        self.previous = current;
        Ok(result)
    }
    pub fn memory(&self) -> Result<f64, Error> {
        let text = std::fs::read_to_string(self.root.join("proc/meminfo"))?;
        let values: BTreeMap<_, _> = text
            .lines()
            .filter_map(|line| {
                let mut parts = line.split_whitespace();
                Some((
                    parts.next()?.trim_end_matches(':'),
                    parts.next()?.parse::<f64>().ok()?,
                ))
            })
            .collect();
        let total = values
            .get("MemTotal")
            .copied()
            .ok_or(Error::Contract("MemTotal missing"))?;
        let available = values
            .get("MemAvailable")
            .copied()
            .ok_or(Error::Contract("MemAvailable missing"))?;
        Ok(((total - available.min(total)) / total * 1000.).round_ties_even() / 10.)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn car_cache_retains_removed_value_and_refreshes_malformed_bytes() {
        // Given a real temporary Params directory and a Tesla CarParams message.
        let root = tempfile::tempdir().unwrap();
        let params = Params(openpilot_params::Params::open(root.path(), "d").unwrap());
        let mut message = capnp::message::Builder::new_default();
        message
            .init_root::<openpilot_cereal::car_capnp::car_params::Builder>()
            .set_brand("tesla");
        params
            .put(
                "CarParams",
                &capnp::serialize::write_message_to_words(&message),
            )
            .unwrap();
        let mut cache = CarCache::default();
        // When Params changes across the source's five-second cache boundary.
        cache.update(&params, 4.9).unwrap();
        assert!(!cache.tesla);
        cache.update(&params, 5.).unwrap();
        assert!(cache.tesla);
        params.remove("CarParams").unwrap();
        cache.update(&params, 10.).unwrap();
        assert!(cache.tesla);
        params.put("CarParams", b"malformed").unwrap();
        cache.update(&params, 14.9).unwrap();
        assert!(cache.tesla);
        cache.update(&params, 15.).unwrap();
        // Then a malformed changed message clears Tesla only on refresh.
        assert!(!cache.tesla);
    }
    #[test]
    fn source_params_boundary_handles_empty_invalid_json_and_io_failures() {
        // Given an owned Params store with no alert value.
        let root = tempfile::tempdir().unwrap();
        let params = Params(openpilot_params::Params::open(root.path(), "d").unwrap());
        // When clearing a missing alert and reading a malformed JSON alert.
        params.remove("Offroad_ConnectivityNeeded").unwrap();
        params
            .put("Offroad_ConnectivityNeeded", b"malformed")
            .unwrap();
        // Then malformed/missing alerts do not create a startup block.
        assert!(startup(&params, 100., true).unwrap()["up_to_date"]);
        params.put("Offroad_ConnectivityNeeded", b"{}").unwrap();
        assert!(!startup(&params, 100., true).unwrap()["up_to_date"]);
        params.put("MaxTimeOffroadMin", b"  +12minutes").unwrap();
        assert_eq!(integer(&params, "MaxTimeOffroadMin").unwrap(), 12);
    }
    #[test]
    fn cpu_usage_subtracts_guest_and_iowait_like_psutil() {
        // Given proc counters in an owned root, including guest and iowait.
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("proc")).unwrap();
        let stat = root.path().join("proc/stat");
        std::fs::write(&stat, "cpu0 100 0 0 100 100 0 0 0 10 0\n").unwrap();
        let mut usage = Usage::new(root.path()).unwrap();
        std::fs::write(stat, "cpu0 120 0 0 120 120 0 0 0 20 0\n").unwrap();
        // When sampling again, guest remains included only once and iowait is idle.
        let actual = usage.cpu().unwrap();
        // Then 20 of 60 ticks are busy.
        assert_eq!(actual, [33.3]);
    }
}
