use crate::{Error, Value};

pub(super) fn text(value: &Value) -> Result<String, Error> {
    Ok(match value {
        Value::Text(points) => crate::state_json::utf8_text(points, "")?,
        Value::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(text)
                .collect::<Result<Vec<_>, _>>()?
                .join(",")
        ),
        Value::Object(fields) => {
            let mut values = Vec::with_capacity(fields.len());
            for (key, value) in fields {
                values.push(format!(
                    "{}:{}",
                    crate::state_json::utf8_text(key, "")?,
                    text(value)?
                ));
            }
            format!("{{{}}}", values.join(","))
        }
        Value::Null | Value::Bool(_) | Value::Integer(_) | Value::Float(_) => value.encode()?,
    })
}
pub(super) fn session(status: &str, code: &str) -> Result<String, Error> {
    text(&Value::object([
        ("type", Value::text("carrotNaviSession")),
        ("version", Value::integer(1)),
        ("status", Value::text(status)),
        ("code", Value::text(code)),
    ]))
}
pub(super) fn frame(metadata: &Value, payload: &[u8]) -> Result<Vec<u8>, Error> {
    let header = text(metadata)?;
    let length = u32::try_from(header.len()).map_err(|error| Error::Source(error.to_string()))?;
    let mut wire = Vec::with_capacity(9 + header.len() + payload.len());
    wire.extend_from_slice(b"CNWB\x01");
    wire.extend_from_slice(&length.to_be_bytes());
    wire.extend_from_slice(header.as_bytes());
    wire.extend_from_slice(payload);
    Ok(wire)
}
pub(super) fn update<const N: usize>(value: &mut Value, changes: [(&str, Value); N]) {
    if let Value::Object(fields) = value {
        for (key, value) in changes {
            let key: Vec<u32> = key.chars().map(u32::from).collect();
            crate::json_fields::insert(fields, key, value);
        }
    }
}
