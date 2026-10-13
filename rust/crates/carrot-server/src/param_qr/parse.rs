use super::{base, binary, compression, error, text, Codec};
use crate::{Error, Value};
use num_bigint::BigInt;
use sha2::{Digest, Sha256};

fn checked(bytes: &[u8], checksum: &str, length: usize, uppercase: bool) -> Result<(), Error> {
    let expected = format!("{:x}", Sha256::digest(bytes))[..length].to_owned();
    let matched = if uppercase {
        expected.eq_ignore_ascii_case(checksum)
    } else {
        expected == checksum
    };
    if matched {
        Ok(())
    } else {
        Err(error("QR payload checksum mismatch"))
    }
}

fn v1(raw: &[u8]) -> Result<Value, Error> {
    let envelope = Value::parse(&text::decode(raw)?)?;
    if !matches!(envelope, Value::Object(_)) {
        return Err(error(format!(
            "'{}' object has no attribute 'get'",
            envelope.type_name()
        )));
    }
    if !envelope.get("type").text_eq("params_backup") {
        return Err(error("unsupported QR backup type"));
    }
    let version = if envelope.has("version") {
        envelope.get("version").int()?
    } else {
        BigInt::from(0)
    };
    if version > BigInt::from(1) {
        return Err(error("unsupported QR backup version"));
    }
    if !matches!(envelope.get("values"), Value::Object(_)) {
        return Err(error("bad QR backup values"));
    }
    Ok(envelope.get("values").clone())
}

fn v2(codec: &Codec, raw: &[u8]) -> Result<Value, Error> {
    let envelope = Value::parse(&text::decode(raw)?)?;
    let empty = Value::Object(Vec::new());
    let zero = Value::integer(0);
    let (version, pairs, fallback) = match &envelope {
        Value::Array(items) if items.len() >= 2 => {
            (&items[0], &items[1], items.get(2).unwrap_or(&empty))
        }
        Value::Object(_) => (
            if envelope.has("v") {
                envelope.get("v")
            } else {
                &zero
            },
            envelope.get("d"),
            if envelope.has("n") {
                envelope.get("n")
            } else {
                &empty
            },
        ),
        _ => return Err(error("bad QR backup format")),
    };
    if version.int()? > BigInt::from(4) {
        return Err(error("unsupported QR backup version"));
    }
    let Value::Array(pairs) = pairs else {
        return Err(error("bad QR backup values"));
    };
    let fallback = if matches!(fallback, Value::Null) {
        &empty
    } else {
        fallback
    };
    let Value::Object(fallback) = fallback else {
        return Err(error("bad QR backup fallback"));
    };
    let codes = codec.required_schema()?.string_codes();
    let mut output = Vec::new();
    for item in pairs {
        let Value::Array(item) = item else {
            continue;
        };
        if item.len() != 2 {
            continue;
        }
        let code = item[0].py_string()?;
        if let Value::Text(points) = code {
            if let Some(name) = points
                .iter()
                .copied()
                .map(char::from_u32)
                .collect::<Option<String>>()
                .as_ref()
                .and_then(|code| codes.get(code))
            {
                binary::assign(
                    &mut output,
                    name.chars().map(u32::from).collect(),
                    item[1].clone(),
                );
            }
        }
    }
    for (name, value) in fallback {
        binary::assign(&mut output, name.clone(), value.clone());
    }
    Ok(Value::Object(output))
}

pub(crate) fn payload(codec: &Codec, data: &Value) -> Result<Value, Error> {
    if matches!(data, Value::Object(_)) {
        return Ok(if matches!(data.get("values"), Value::Object(_)) {
            data.get("values")
        } else {
            data
        }
        .clone());
    }
    let Value::Text(points) = (if data.truth() {
        data.py_string()?
    } else {
        Value::text("")
    }) else {
        return Err(error("unsupported QR payload"));
    };
    let points = crate::state::trim(&points);
    if points.is_empty() {
        return Err(error("empty payload"));
    }
    let payload = points
        .iter()
        .copied()
        .map(char::from_u32)
        .collect::<Option<String>>()
        .ok_or_else(|| error("unsupported QR payload"))?;
    if payload.starts_with('{') {
        return self::payload(codec, &Value::parse(&payload)?);
    }
    for (prefix, brotli) in [("CQR3:", true), ("CQR4:", false)] {
        if let Some(body) = payload.strip_prefix(prefix) {
            if brotli && codec.brotli.is_none() {
                return Err(error("No module named 'brotli'"));
            }
            let (encoded, checksum) = body
                .rsplit_once(':')
                .ok_or_else(|| error("bad QR payload"))?;
            let compressed = base::b45_decode(encoded)?;
            checked(&compressed, checksum, 12, true)?;
            let raw = if brotli {
                compression::brotli(&compressed)?
            } else {
                compression::zlib(&compressed)?
            };
            return binary::parse(codec, &raw);
        }
    }
    let parts: Vec<_> = payload.split('.').collect();
    if parts.len() != 3 || !matches!(parts[0], "CQR1" | "CQR2") {
        return Err(error("unsupported QR payload"));
    }
    let compressed = base::b64_decode(parts[1])?;
    checked(
        &compressed,
        parts[2],
        if parts[0] == "CQR1" { 16 } else { 12 },
        false,
    )?;
    let raw = compression::zlib(&compressed)?;
    if parts[0] == "CQR1" {
        v1(&raw)
    } else {
        v2(codec, &raw)
    }
}
