use openpilot_radard::path::cache::{self, Snapshot};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
pub struct Cursor {
    geometry: Vec<usize>,
    projection: Vec<usize>,
}

pub fn restore(cursor: Cursor, archive: &Snapshot) -> Result<(), Box<dyn std::error::Error>> {
    cache::restore(Snapshot {
        geometry: cursor
            .geometry
            .into_iter()
            .map(|index| {
                archive
                    .geometry
                    .get(index)
                    .cloned()
                    .ok_or("geometry archive index missing")
            })
            .collect::<Result<_, _>>()?,
        projection: cursor
            .projection
            .into_iter()
            .map(|index| {
                archive
                    .projection
                    .get(index)
                    .cloned()
                    .ok_or("projection archive index missing")
            })
            .collect::<Result<_, _>>()?,
    })?;
    Ok(())
}

fn float_bits(value: Value) -> Result<Value, Box<dyn std::error::Error>> {
    Ok(match value {
        Value::Number(value) => Value::String(format!(
            "{:016x}",
            value.as_f64().ok_or("cache number is not float")?.to_bits()
        )),
        Value::Array(values) => Value::Array(
            values
                .into_iter()
                .map(float_bits)
                .collect::<Result<_, _>>()?,
        ),
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, value)| Ok((key, float_bits(value)?)))
                .collect::<Result<_, Box<dyn std::error::Error>>>()?,
        ),
        Value::String(value) => Value::String(value),
        Value::Bool(value) => Value::Bool(value),
        Value::Null => Value::Null,
    })
}

pub fn fingerprints() -> Result<Value, Box<dyn std::error::Error>> {
    let snapshot = serde_json::to_value(cache::snapshot())?;
    let mut output = serde_json::Map::new();
    for name in ["geometry", "projection"] {
        let values = snapshot
            .get(name)
            .and_then(Value::as_array)
            .ok_or("cache snapshot table missing")?;
        let mut fingerprints = Vec::new();
        for value in values {
            let encoded = serde_json::to_vec(&float_bits(value.clone())?)?;
            fingerprints.push(Value::String(format!("{:x}", Sha256::digest(encoded))));
        }
        output.insert(name.to_owned(), Value::Array(fingerprints));
    }
    Ok(Value::Object(output))
}
