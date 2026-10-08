use super::{base, binary, error, schema::Schema, text, zlib, Codec};
use crate::{Error, Value};
use sha2::{Digest, Sha256};

fn envelope(codec: &Codec, input: &Value) -> Result<Vec<u8>, Error> {
    let fallback_schema;
    let schema = match &codec.schema {
        Some(schema) => schema,
        None => {
            fallback_schema = Schema::for_values(input)?;
            &fallback_schema
        }
    };
    let codes = schema.key_codes(3);
    let mut fields: Vec<_> = crate::json_fields::fields(input)?.iter().collect();
    fields.sort_by(|left, right| left.0.cmp(&right.0));
    let mut pairs = Vec::new();
    let mut fallback = Vec::new();
    for (key, value) in fields {
        let name = key
            .iter()
            .copied()
            .map(char::from_u32)
            .collect::<Option<String>>();
        if let Some(code) = name.as_ref().and_then(|name| codes.get(name)) {
            pairs.push(Value::Array(vec![
                Value::text(&base::b64_encode(code)),
                value.clone(),
            ]));
        } else {
            fallback.push((key.clone(), value.clone()));
        }
    }
    let mut envelope = vec![Value::integer(2), Value::Array(pairs)];
    if !fallback.is_empty() {
        envelope.push(Value::Object(fallback));
    }
    text::compact(&Value::Array(envelope))
}

pub(crate) fn payload(codec: &Codec, values: &Value, version: u8) -> Result<Value, Error> {
    let (raw, compressed) = match version {
        2 => {
            let raw = envelope(codec, values)?;
            let compressed = zlib::compress(&raw)?;
            (raw, compressed)
        }
        3 => {
            let brotli = codec
                .brotli
                .as_ref()
                .ok_or_else(|| error("No module named 'brotli'"))?;
            let raw = binary::build(codec, values, version)?;
            let compressed = brotli.compress(&raw)?;
            (raw, compressed)
        }
        4 => {
            let raw = binary::build(codec, values, version)?;
            let compressed = zlib::compress(&raw)?;
            (raw, compressed)
        }
        _ => return Err(error("unsupported QR backup version")),
    };
    let format = format!("CQR{version}");
    let mut checksum = format!("{:x}", Sha256::digest(&compressed))[..12].to_owned();
    let payload = if version == 2 {
        format!("{format}.{}.{checksum}", base::b64_encode(&compressed))
    } else {
        checksum.make_ascii_uppercase();
        format!("{format}:{}:{checksum}", base::b45_encode(&compressed))
    };
    Ok(Value::object([
        ("payload", Value::text(&payload)),
        ("format", Value::text(&format)),
        (
            "count",
            Value::integer(crate::json_fields::fields(values)?.len()),
        ),
        ("json_bytes", Value::integer(raw.len())),
        ("compressed_bytes", Value::integer(compressed.len())),
        ("payload_chars", Value::integer(payload.chars().count())),
        ("ecc", Value::text("L")),
        ("version", Value::integer(version)),
        ("checksum", Value::text(&checksum)),
    ]))
}
