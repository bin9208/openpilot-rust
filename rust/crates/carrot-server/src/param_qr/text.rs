use super::error;
use crate::{Error, Value};

pub(crate) fn utf8(points: &[u32]) -> Result<String, Error> {
    let mut out = String::new();
    for (index, point) in points.iter().copied().enumerate() {
        let Some(character) = char::from_u32(point) else {
            let count = points[index..]
                .iter()
                .take_while(|point| (0xd800..=0xdfff).contains(*point))
                .count();
            let reason = if count == 1 {
                format!("character '\\u{point:04x}' in position {index}")
            } else {
                format!("characters in position {index}-{}", index + count - 1)
            };
            return Err(error(format!(
                "'utf-8' codec can't encode {reason}: surrogates not allowed"
            )));
        };
        out.push(character);
    }
    Ok(out)
}

pub(crate) fn decode(bytes: &[u8]) -> Result<String, Error> {
    std::str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|failure| {
            let start = failure.valid_up_to();
            let length = failure.error_len().unwrap_or(bytes.len() - start);
            let reason = match failure.error_len() {
                None => "unexpected end of data",
                Some(_) if (0xc2..=0xf4).contains(&bytes[start]) => "invalid continuation byte",
                Some(_) => "invalid start byte",
            };
            let subject = if length == 1 {
                format!("byte 0x{:02x} in position {start}", bytes[start])
            } else {
                format!("bytes in position {start}-{}", start + length - 1)
            };
            error(format!("'utf-8' codec can't decode {subject}: {reason}"))
        })
}

fn json_points(value: &Value, out: &mut Vec<u32>) -> Result<(), Error> {
    match value {
        Value::Text(points) => {
            out.push(34);
            for point in points {
                let escaped = match *point {
                    34 => Some("\\\""),
                    92 => Some("\\\\"),
                    8 => Some("\\b"),
                    12 => Some("\\f"),
                    10 => Some("\\n"),
                    13 => Some("\\r"),
                    9 => Some("\\t"),
                    _ => None,
                };
                if let Some(escaped) = escaped {
                    out.extend(escaped.chars().map(u32::from));
                } else if *point < 32 {
                    out.extend(format!("\\u{point:04x}").chars().map(u32::from));
                } else {
                    out.push(*point);
                }
            }
            out.push(34);
        }
        Value::Array(values) => {
            out.push(91);
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    out.push(44);
                }
                json_points(value, out)?;
            }
            out.push(93);
        }
        Value::Object(fields) => {
            out.push(123);
            for (index, (name, value)) in fields.iter().enumerate() {
                if index > 0 {
                    out.push(44);
                }
                json_points(&Value::Text(name.clone()), out)?;
                out.push(58);
                json_points(value, out)?;
            }
            out.push(125);
        }
        Value::Null | Value::Bool(_) | Value::Integer(_) | Value::Float(_) => {
            out.extend(value.encode()?.chars().map(u32::from));
        }
    }
    Ok(())
}

pub(crate) fn compact(value: &Value) -> Result<Vec<u8>, Error> {
    let mut points = Vec::new();
    json_points(value, &mut points)?;
    Ok(utf8(&points)?.into_bytes())
}
