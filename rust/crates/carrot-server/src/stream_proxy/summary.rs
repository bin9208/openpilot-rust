use crate::{Error, Value};

pub(super) fn parse(bytes: &[u8]) -> Value {
    let parsed = || -> Result<Value, Error> {
        let value = Value::parse(&String::from_utf8_lossy(bytes))?;
        if !matches!(value, Value::Object(_)) {
            return Ok(Value::object([]));
        }
        let string = |name| -> Result<Value, Error> {
            Ok(if value.get(name).truth() {
                value.get(name).py_string()?
            } else {
                Value::text("")
            })
        };
        let client = match string("client_id")? {
            Value::Text(points) => Value::Text(points.into_iter().take(128).collect()),
            other => other,
        };
        let size = match string("sdp")? {
            Value::Text(points) => points.len(),
            _ => 0,
        };
        Ok(Value::object([
            ("cameras", value.get("cameras").clone()),
            (
                "bridge_services_in",
                value.get("bridge_services_in").clone(),
            ),
            (
                "bridge_services_out",
                value.get("bridge_services_out").clone(),
            ),
            ("client_id", client),
            ("takeover", Value::Bool(value.get("takeover").truth())),
            (
                "carrot_state",
                Value::Bool(value.get("carrot_state").truth()),
            ),
            ("sdp_bytes", Value::integer(size)),
        ]))
    };
    parsed().unwrap_or_else(|_| Value::object([]))
}
