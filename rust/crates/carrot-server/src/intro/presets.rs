use super::Intro;
use crate::{param_coercion::rounded, params::Backend, Error, Value};
use hyper::StatusCode;

pub const NAMES: [&str; 3] = ["radar_long", "camera_long", "stock"];
const PARAMS: [&str; 7] = [
    "HyundaiCameraSCC",
    "SpeedFromPCM",
    "DisableDM",
    "EnableRadarTracks",
    "EnableCornerRadar",
    "AutoCruiseControl",
    "AutoEngage",
];

fn preset(name: &Value) -> Option<(&'static str, [i64; 7])> {
    if name.text_eq("radar_long") {
        Some(("radar_long", [1, 0, 0, 0, 1, 1, 2]))
    } else if name.text_eq("camera_long") {
        Some(("camera_long", [1, 0, 0, 0, 0, 1, 2]))
    } else if name.text_eq("stock") {
        Some(("stock", [0, 2, 0, 0, 0, 0, 2]))
    } else {
        None
    }
}

fn numeric(value: &Value) -> bool {
    matches!(value, Value::Bool(_) | Value::Integer(_) | Value::Float(_))
}
fn integer(value: &Value) -> bool {
    matches!(value, Value::Bool(_) | Value::Integer(_))
}

fn clamped(value: Value, definition: Option<&Value>) -> Value {
    let Some(definition) = definition else {
        return value;
    };
    if !numeric(definition.get("min")) || !numeric(definition.get("max")) {
        return value;
    }
    let result = (|| -> Result<Value, Error> {
        let mut number = value.float()?;
        let minimum = definition.get("min").float()?;
        let maximum = definition.get("max").float()?;
        if number < minimum {
            number = minimum;
        }
        if number > maximum {
            number = maximum;
        }
        let clamped = Value::Float(number);
        if [
            definition.get("min"),
            definition.get("max"),
            definition.get("default"),
        ]
        .iter()
        .all(|value| integer(value))
        {
            return Ok(Value::Integer(rounded(&clamped)?));
        }
        Ok(clamped)
    })();
    result.unwrap_or(value)
}

impl Intro {
    pub fn apply_preset(
        &self,
        params: &mut Backend,
        name: &Value,
    ) -> Result<(StatusCode, Value), Error> {
        if !params.has_params() {
            return Ok((
                StatusCode::INTERNAL_SERVER_ERROR,
                Value::object([
                    ("ok", Value::Bool(false)),
                    ("error", Value::text("Params not available")),
                ]),
            ));
        }
        let name = crate::param_changes::text::stripped(name, true)?;
        let Some((name, values)) = preset(&name) else {
            let Value::Text(name) = name else {
                return Err(Error::Source("intro preset name is not text".into()));
            };
            let mut message: Vec<u32> = "unknown preset: ".chars().map(u32::from).collect();
            message.extend(name);
            return Ok((
                StatusCode::BAD_REQUEST,
                Value::object([
                    ("ok", Value::Bool(false)),
                    ("error", Value::Text(message)),
                    (
                        "known",
                        Value::Array(NAMES.into_iter().map(Value::text).collect()),
                    ),
                ]),
            ));
        };
        let mut applied = Vec::new();
        let mut failed = Vec::new();
        for (name, raw) in PARAMS.into_iter().zip(values) {
            let definitions = self.definitions(params).ok();
            let definition = definitions
                .as_ref()
                .map(|definitions| definitions.get(name))
                .filter(|definition| !matches!(definition, Value::Null));
            let value = clamped(Value::integer(raw), definition);
            let key = name.chars().map(u32::from).collect();
            match params.put(name, &value, definition) {
                Ok(()) => applied.push((key, value)),
                Err(error) => failed.push((key, Value::text(&error.to_string()))),
            }
        }
        let ok = failed.is_empty();
        let mut response = Value::object([("ok", Value::Bool(ok))]);
        if !ok {
            crate::json_fields::set(&mut response, "error", Value::text("some params failed"))?;
        }
        for (name, value) in [
            ("preset", Value::text(name)),
            ("applied", Value::Object(applied)),
            ("failed", Value::Object(failed)),
        ] {
            crate::json_fields::set(&mut response, name, value)?;
        }
        Ok((
            if ok {
                StatusCode::OK
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            },
            response,
        ))
    }
}
