use crate::{Error, Value};

pub(super) fn failure(status: Option<u16>, message: Value) -> Value {
    Value::object([
        ("online_ok", Value::Bool(false)),
        (
            "http_status",
            status.map(Value::integer).unwrap_or(Value::Null),
        ),
        ("message", message),
    ])
}

fn mapping(bytes: &[u8]) -> Result<Value, Error> {
    if bytes.is_empty() {
        return Ok(Value::Object(Vec::new()));
    }
    Ok(Value::parse(&String::from_utf8_lossy(bytes))?)
}

fn message(data: &Value, fallback: Value) -> Value {
    if data.get("message").truth() {
        data.get("message").clone()
    } else if data.get("code").truth() {
        data.get("code").clone()
    } else {
        fallback
    }
}

pub(super) fn result(status: u16, bytes: &[u8]) -> Value {
    if !(200..300).contains(&status) {
        let fallback = Value::text(&format!("HTTP {status}"));
        let message = mapping(bytes)
            .map(|data| message(&data, fallback.clone()))
            .unwrap_or(fallback);
        return failure(Some(status), message);
    }
    match success(status, bytes) {
        Ok(value) => value,
        Err(error) => failure(None, Value::text(&error.to_string())),
    }
}

fn success(status: u16, bytes: &[u8]) -> Result<Value, Error> {
    let data = mapping(bytes)?;
    if !matches!(data, Value::Object(_)) {
        return Err(Error::Source(format!(
            "'{}' object has no attribute 'get'",
            data.type_name()
        )));
    }
    let empty = Value::text("");
    let code = if data.has("code") {
        data.get("code")
    } else {
        &empty
    };
    let lower = super::policy::lower(&code.py_string()?)?;
    let ok = !lower.truth() || lower.text_eq("ok");
    Ok(Value::object([
        ("online_ok", Value::Bool(ok)),
        ("http_status", Value::integer(status)),
        (
            "message",
            if ok {
                Value::text("Mapbox Directions API is reachable.")
            } else {
                message(&data, Value::text("Mapbox validation failed."))
            },
        ),
    ]))
}
