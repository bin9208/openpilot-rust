use crate::{Error, Value};
use sha2::{Digest, Sha256};

pub(crate) fn utf8(value: &Value) -> Result<String, Error> {
    openpilot_logmessaged::JsonValue::parse(&value.encode()?)
        .map_err(|error| Error::Source(error.to_string()))?
        .to_json_utf8()
        .map_err(|_| Error::Source("cannot encode lone surrogate as UTF-8".into()))
}

pub fn canonical(value: &Value) -> Result<String, Error> {
    match value {
        Value::Array(items) => Ok(format!(
            "[{}]",
            items
                .iter()
                .map(canonical)
                .collect::<Result<Vec<_>, _>>()?
                .join(",")
        )),
        Value::Object(fields) => {
            let mut fields: Vec<_> = fields.iter().collect();
            fields.sort_by(|left, right| left.0.cmp(&right.0));
            let pairs = fields
                .into_iter()
                .map(|(name, value)| {
                    Ok(format!(
                        "{}:{}",
                        utf8(&Value::Text(name.clone()))?,
                        canonical(value)?
                    ))
                })
                .collect::<Result<Vec<_>, Error>>()?;
            Ok(format!("{{{}}}", pairs.join(",")))
        }
        Value::Text(_) => utf8(value),
        Value::Null | Value::Bool(_) | Value::Integer(_) | Value::Float(_) => Ok(value.encode()?),
    }
}

pub fn record_hash(record: &Value) -> Result<String, Error> {
    let body = Value::object([
        ("ts", record.get("ts").clone()),
        ("name", record.get("name").clone()),
        ("prev", record.get("prev").clone()),
        ("next", record.get("next").clone()),
        ("source", record.get("source").clone()),
        ("engaged", record.get("engaged").clone()),
        (
            "prev_hash",
            if record.has("prev_hash") {
                record.get("prev_hash").clone()
            } else {
                Value::text(super::GENESIS_HASH)
            },
        ),
    ]);
    let encoded = canonical(&body)?;
    Ok(format!("{:x}", Sha256::digest(encoded.as_bytes())))
}

pub fn fingerprint(values: &Value) -> Result<Value, Error> {
    let fields = if values.truth() {
        crate::json_fields::fields(values)?.clone()
    } else {
        Vec::new()
    };
    let count = fields.len();
    let digest = format!(
        "{:x}",
        Sha256::digest(canonical(&Value::Object(fields))?.as_bytes())
    );
    Ok(Value::object([
        ("fingerprint", Value::text(&digest[..8])),
        ("digest", Value::text(&digest)),
        ("count", Value::integer(count)),
    ]))
}
