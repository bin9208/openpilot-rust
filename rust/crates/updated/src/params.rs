use crate::Error;
use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, Timelike, Utc};
use openpilot_logging::producer::Logger;
use openpilot_params::Params as NativeParams;
use std::path::Path;

pub struct Params(pub NativeParams);
#[derive(Debug)]
pub enum ParamTime {
    Naive(NaiveDateTime),
    Aware(DateTime<FixedOffset>),
}
impl Params {
    pub fn open(system_root: &Path) -> Result<Self, Error> {
        Ok(Self(if system_root == Path::new("/") {
            NativeParams::for_runtime()?
        } else {
            NativeParams::open(&system_root.join("data/params"), "d")?
        }))
    }
    pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>, Error> {
        match self.0.get(key) {
            Ok(value) => Ok(value.filter(|v| !v.is_empty())),
            Err(openpilot_params::Error::Io(_)) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
    pub fn text(&self, key: &str, logger: &mut Logger) -> Result<Option<String>, Error> {
        Ok(openpilot_params_typed::get_string(&self.0, key, logger)?)
    }
    pub fn boolean(&self, key: &str) -> Result<bool, Error> {
        Ok(self.get(key)?.as_deref() == Some(b"1"))
    }
    pub fn put_bool(&self, key: &str, value: bool) -> Result<(), Error> {
        self.put(key, if value { b"1" } else { b"0" })
    }
    pub fn put(&self, key: &str, value: &[u8]) -> Result<(), Error> {
        Self::write_result(self.0.put(key, value))
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
    pub fn integer(&self, key: &str) -> Result<i128, Error> {
        let default = openpilot_params::metadata(key)
            .and_then(|value| value.default)
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        let Some(bytes) = self.get(key)? else {
            return Ok(default);
        };
        let Ok(text) = std::str::from_utf8(&bytes) else {
            return Ok(default);
        };
        let text = text.trim();
        let valid = text.as_bytes().iter().enumerate().all(|(index, byte)| {
            *byte != b'_'
                || (index > 0
                    && text.as_bytes()[index - 1].is_ascii_digit()
                    && text
                        .as_bytes()
                        .get(index + 1)
                        .is_some_and(u8::is_ascii_digit))
        });
        Ok(if valid {
            text.replace('_', "").parse().unwrap_or(default)
        } else {
            default
        })
    }
    pub fn number(&self, key: &str) -> Result<f64, Error> {
        let default = openpilot_params::metadata(key)
            .and_then(|v| v.default)
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.);
        Ok(self
            .get(key)?
            .as_deref()
            .and_then(|v| std::str::from_utf8(v).ok())
            .and_then(|v| v.trim().parse::<f64>().ok())
            .unwrap_or(default))
    }
    pub fn put_number(&self, key: &str, value: f64) -> Result<(), Error> {
        let kind = openpilot_params::metadata(key)
            .ok_or(Error::Contract("unknown numeric Param"))?
            .kind;
        let text = if kind == 2 {
            format!("{:.0}", value.trunc())
        } else {
            python_float(value)?
        };
        self.put(key, text.as_bytes())
    }
    pub fn date(&self, key: &str) -> Result<Option<ParamTime>, Error> {
        let Some(bytes) = self.get(key)? else {
            return Ok(None);
        };
        let Ok(text) = std::str::from_utf8(&bytes) else {
            return Ok(None);
        };
        let naive = NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S%.f")
            .ok()
            .or_else(|| NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S%.f").ok())
            .or_else(|| {
                NaiveDate::parse_from_str(text, "%Y-%m-%d")
                    .ok()
                    .and_then(|date| date.and_hms_opt(0, 0, 0))
            });
        if let Some(value) = naive {
            return Ok(value
                .with_nanosecond(value.nanosecond() / 1000 * 1000)
                .map(ParamTime::Naive));
        }
        Ok(DateTime::parse_from_rfc3339(text)
            .ok()
            .map(ParamTime::Aware))
    }
    pub fn put_date(&self, key: &str, time: DateTime<Utc>) -> Result<(), Error> {
        self.put(
            key,
            time.naive_utc()
                .format(if time.nanosecond() == 0 {
                    "%Y-%m-%dT%H:%M:%S"
                } else {
                    "%Y-%m-%dT%H:%M:%S%.6f"
                })
                .to_string()
                .as_bytes(),
        )
    }
    pub fn alert(&self, key: &str, show: bool, extra: Option<&str>) -> Result<(), Error> {
        if !show {
            return self.remove(key);
        }
        use openpilot_logmessaged::{JsonValue, JsonView};
        let alerts = JsonValue::parse(include_str!(
            "../../../../openpilot/selfdrive/selfdrived/alerts_offroad.json"
        ))?;
        let alert = alerts
            .get(key)
            .ok_or(Error::Contract("unknown offroad alert"))?;
        let JsonView::Object(fields) = alert.view() else {
            return Err(Error::Contract("offroad alert object"));
        };
        let mut parts = Vec::new();
        for (name, value) in fields {
            let name =
                JsonValue::codepoints(name.to_vec()).ok_or(Error::Contract("alert field name"))?;
            if !name.text_eq("extra") {
                parts.push(format!("{}: {}", name.to_json()?, value.to_json()?));
            }
        }
        parts.push(format!(
            "\"extra\": {}",
            JsonValue::text(extra.unwrap_or("")).to_json()?
        ));
        self.put(key, format!("{{{}}}", parts.join(", ")).as_bytes())
    }
}

pub fn python_float(value: f64) -> Result<String, Error> {
    let mut text = String::new();
    openpilot_runtime_core::python_float::write_float(value, &mut text)
        .map_err(|_| Error::Contract("float formatting"))?;
    Ok(text)
}
