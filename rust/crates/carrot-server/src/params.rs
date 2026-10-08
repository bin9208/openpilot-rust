use crate::param_coercion::{bool_value, python_float, rounded};
pub use crate::param_coercion::{coerce_inferred, infer_type};
use crate::{native, Error, Value};
use num_traits::ToPrimitive;
use openpilot_params::{metadata, Params};
use std::{collections::HashMap, fs, io::Write, path::PathBuf};

pub struct Backend {
    native: Option<Params>,
    memory: HashMap<String, String>,
    state: PathBuf,
}

fn default_value(default: &Value) -> Value {
    if matches!(default, Value::Null) {
        Value::text("")
    } else {
        default.clone()
    }
}

impl Backend {
    pub fn native(params: Params, state: PathBuf) -> Self {
        Self {
            native: Some(params),
            memory: HashMap::new(),
            state,
        }
    }

    pub fn memory(state: PathBuf) -> Self {
        Self {
            native: None,
            memory: HashMap::new(),
            state,
        }
    }

    pub fn has_params(&self) -> bool {
        self.native.is_some()
    }

    fn custom_value(&self, name: &str) -> Option<Value> {
        if name != "GitPullTime" {
            return None;
        }
        let source = fs::read_to_string(self.state.join("git.json")).ok()?;
        let data = Value::parse(&source).ok()?;
        let value = data.get("git_pull_time");
        if matches!(value, Value::Null) {
            return None;
        }
        Some(Value::text(value.string().ok()?.trim()))
    }

    fn raw_unknown(params: &Params, name: &str) -> Result<PathBuf, Error> {
        if name.is_empty()
            || !name.is_ascii()
            || !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
        {
            return Err(Error::Source(format!("invalid param name: {name}")));
        }
        Ok(params.directory().join(name))
    }

    pub fn get(&self, name: &str, default: &Value) -> Value {
        if let Some(value) = self.custom_value(name) {
            return value;
        }
        let Some(params) = &self.native else {
            return self
                .memory
                .get(name)
                .map_or_else(|| default.clone(), |value| Value::text(value));
        };
        if let Some(info) = metadata(name) {
            let bytes = params.get(name).ok().flatten().unwrap_or_default();
            if info.kind == 1 {
                return Value::Bool(bytes == b"1");
            }
            if info.kind == 2 {
                if let Ok(value) = openpilot_beepd::integer(&bytes) {
                    return Value::integer(value);
                }
            }
            if info.kind == 3 {
                if let Ok(value) = openpilot_calibrationd::parameters::parse_float(&bytes) {
                    return Value::Float(value);
                }
            }
            if bytes.is_empty() {
                return default_value(default);
            }
            if info.kind == 6 {
                return Value::text(&String::from_utf8_lossy(&bytes));
            }
            let Ok(text) = String::from_utf8(bytes) else {
                return default_value(default);
            };
            let value = match info.kind {
                2 => Value::text(&text).int().map(Value::Integer),
                3 => Value::text(&text).float().map(Value::Float),
                5 => Value::parse(&text),
                _ => return Value::text(&text),
            };
            return value
                .and_then(|value| value.py_string())
                .unwrap_or_else(|_| default_value(default));
        }
        let value = Self::raw_unknown(params, name).and_then(|path| Ok(fs::read(path)?));
        let Ok(bytes) = value else {
            return default_value(default);
        };
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let raw = Value::text(&text);
        match default {
            Value::Bool(_) => bool_value(&raw).map(Value::Bool),
            Value::Integer(_) => rounded(&raw).map(Value::Integer),
            Value::Float(_) => raw.float().map(Value::Float).map_err(Error::from),
            Value::Array(_) | Value::Object(_) => Value::parse(&text).map_err(Error::from),
            Value::Null | Value::Text(_) => return raw,
        }
        .unwrap_or_else(|_| default_value(default))
    }

    pub fn maximum_gap_levels(&self) -> i64 {
        if self.native.is_none() {
            return 4;
        }
        self.get("LongitudinalPersonalityMax", &Value::integer(0))
            .int()
            .ok()
            .and_then(|v| v.to_i64())
            .filter(|value| matches!(value, 3 | 4))
            .unwrap_or(3)
    }

    pub fn vehicle_brand(&self) -> String {
        let Some(params) = &self.native else {
            return String::new();
        };
        if let Ok(Some(bytes)) = params.get("CarParamsPersistent") {
            let brand = (|| {
                let reader = capnp::serialize::read_message(
                    &mut std::io::Cursor::new(bytes),
                    Default::default(),
                )
                .ok()?;
                let car = reader
                    .get_root::<openpilot_cereal::car_capnp::car_params::Reader<'_>>()
                    .ok()?;
                Some(car.get_brand().ok()?.to_str().ok()?.trim().to_lowercase())
            })();
            if let Some(brand) = brand.filter(|brand| !brand.is_empty()) {
                return brand;
            }
        }
        let car = self
            .get("CarName", &Value::text(""))
            .string()
            .unwrap_or_default()
            .trim()
            .to_uppercase();
        if ["HYUNDAI", "KIA", "GENESIS"]
            .iter()
            .any(|brand| car.starts_with(brand))
        {
            "hyundai".into()
        } else {
            String::new()
        }
    }

    pub fn put(&mut self, name: &str, value: &Value, setting: Option<&Value>) -> Result<(), Error> {
        let Some(params) = &self.native else {
            self.memory.insert(name.into(), value.string()?);
            return Ok(());
        };
        let kind = metadata(name).map(|info| info.kind);
        let inferred = setting.map(infer_type).unwrap_or("string");
        let raw = match kind {
            Some(1) => if bool_value(value)? { "1" } else { "0" }.into(),
            Some(2) => {
                let number = rounded(value)?
                    .to_i32()
                    .ok_or_else(|| Error::Source("value too large to convert to int".into()))?;
                number.to_string()
            }
            Some(3) => native::float_text(python_float(value)? as f32)?,
            Some(5) => {
                let value = if matches!(value, Value::Text(_)) {
                    Value::parse(&value.string()?)?
                } else {
                    value.clone()
                };
                value.encode()?
            }
            Some(_) => value.string()?,
            None => match inferred {
                "bool" => if bool_value(value)? { "1" } else { "0" }.into(),
                "int" => rounded(value)?.to_string(),
                "float" => Value::Float(python_float(value)?).string()?,
                _ => value.string()?,
            },
        };
        if kind.is_some() {
            return Ok(params.put(name, raw.as_bytes())?);
        }
        if setting.is_none() {
            return Err(openpilot_params::Error::UnknownKey(name.into()).into());
        }
        let destination = Self::raw_unknown(params, name)?;
        let parent = params
            .directory()
            .parent()
            .ok_or_else(|| Error::Source("invalid Params directory".into()))?;
        let mut temporary = tempfile::Builder::new()
            .prefix(".tmp_value_")
            .tempfile_in(parent)?;
        temporary.write_all(raw.as_bytes())?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(destination)
            .map_err(|error| Error::Io(error.error))?;
        if let Ok(directory) = fs::File::open(params.directory()) {
            let _ = directory.sync_all();
        }
        Ok(())
    }
}
