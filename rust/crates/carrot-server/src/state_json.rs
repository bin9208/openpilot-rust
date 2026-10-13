use crate::{Error, Value};
use std::{fs::File, io::Write, path::Path};

pub(crate) fn compact_encoded(encoded: &str) -> String {
    let mut quoted = false;
    let mut escaped = false;
    encoded
        .chars()
        .filter(|c| {
            if quoted {
                if escaped {
                    escaped = false;
                } else if *c == '\\' {
                    escaped = true;
                } else if *c == '"' {
                    quoted = false;
                }
                true
            } else if *c == '"' {
                quoted = true;
                true
            } else {
                !c.is_whitespace()
            }
        })
        .collect()
}

pub(crate) fn utf8_text(points: &[u32], prefix: &str) -> Result<String, Error> {
    let mut text = String::from(prefix);
    text.push('"');
    for (index, point) in points.iter().enumerate() {
        match *point {
            34 => text.push_str("\\\""),
            92 => text.push_str("\\\\"),
            8 => text.push_str("\\b"),
            12 => text.push_str("\\f"),
            10 => text.push_str("\\n"),
            13 => text.push_str("\\r"),
            9 => text.push_str("\\t"),
            0..=31 => {
                use std::fmt::Write;
                write!(text, "\\u{point:04x}")
                    .map_err(|_| Error::Source("text formatting failed".into()))?;
            }
            point => {
                let Some(character) = char::from_u32(point) else {
                    let start = text.chars().count();
                    let count = points[index..]
                        .iter()
                        .take_while(|next| (0xd800..=0xdfff).contains(*next))
                        .count();
                    let reason = if count == 1 {
                        format!("character '\\u{point:04x}' in position {start}")
                    } else {
                        format!("characters in position {start}-{}", start + count - 1)
                    };
                    return Err(Error::Source(format!(
                        "'utf-8' codec can't encode {reason}: surrogates not allowed"
                    )));
                };
                text.push(character);
            }
        }
    }
    text.push('"');
    Ok(text)
}

fn write_value(file: &mut File, value: &Value, depth: usize) -> Result<(), Error> {
    match value {
        Value::Text(points) => file.write_all(utf8_text(points, "")?.as_bytes())?,
        Value::Array(items) if !items.is_empty() => {
            for (index, item) in items.iter().enumerate() {
                let prefix = format!(
                    "{}\n{}",
                    if index == 0 { "[" } else { "," },
                    "  ".repeat(depth + 1)
                );
                match item {
                    Value::Text(points) => {
                        file.write_all(utf8_text(points, &prefix)?.as_bytes())?
                    }
                    Value::Null
                    | Value::Bool(_)
                    | Value::Integer(_)
                    | Value::Float(_)
                    | Value::Array(_)
                    | Value::Object(_) => {
                        file.write_all(prefix.as_bytes())?;
                        write_value(file, item, depth + 1)?;
                    }
                }
            }
            file.write_all(format!("\n{}]", "  ".repeat(depth)).as_bytes())?;
        }
        Value::Object(fields) if !fields.is_empty() => {
            let mut fields: Vec<_> = fields.iter().collect();
            fields.sort_by(|(first, _), (second, _)| first.cmp(second));
            file.write_all(b"{")?;
            for (index, (key, value)) in fields.into_iter().enumerate() {
                if index > 0 {
                    file.write_all(b",")?;
                }
                file.write_all(format!("\n{}", "  ".repeat(depth + 1)).as_bytes())?;
                file.write_all(utf8_text(key, "")?.as_bytes())?;
                file.write_all(b": ")?;
                write_value(file, value, depth + 1)?;
            }
            file.write_all(format!("\n{}}}", "  ".repeat(depth)).as_bytes())?;
        }
        Value::Null
        | Value::Bool(_)
        | Value::Integer(_)
        | Value::Float(_)
        | Value::Array(_)
        | Value::Object(_) => file.write_all(value.encode()?.as_bytes())?,
    }
    Ok(())
}

pub(crate) fn write_json(path: &Path, value: &Value) -> Result<(), Error> {
    let mut file = File::create(path)?;
    write_value(&mut file, value, 0)?;
    file.write_all(b"\n")?;
    Ok(())
}
