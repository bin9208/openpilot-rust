use super::Value;
use crate::Error;
use std::fmt::Write;
use unicode_general_category::{get_general_category, GeneralCategory};

const _: () = assert!(unicode_general_category::UNICODE_VERSION.0 == 15);

fn printable(point: u32) -> bool {
    if point == 32 {
        return true;
    }
    char::from_u32(point).is_some_and(|character| {
        !matches!(
            get_general_category(character),
            GeneralCategory::Control
                | GeneralCategory::Format
                | GeneralCategory::Surrogate
                | GeneralCategory::PrivateUse
                | GeneralCategory::Unassigned
                | GeneralCategory::LineSeparator
                | GeneralCategory::ParagraphSeparator
                | GeneralCategory::SpaceSeparator
        )
    })
}

fn text(points: &[u32]) -> Result<String, Error> {
    let quote = if points.contains(&39) && !points.contains(&34) {
        '"'
    } else {
        '\''
    };
    let mut output = String::with_capacity(points.len() + 2);
    output.push(quote);
    for &point in points {
        match point {
            9 => output.push_str("\\t"),
            10 => output.push_str("\\n"),
            13 => output.push_str("\\r"),
            92 => output.push_str("\\\\"),
            point if point == u32::from(quote) => {
                output.push('\\');
                output.push(quote);
            }
            point if printable(point) => output
                .push(char::from_u32(point).ok_or_else(|| Error::value("invalid Python text"))?),
            point => {
                if point <= 255 {
                    write!(output, "\\x{point:02x}")
                } else if point <= 65535 {
                    write!(output, "\\u{point:04x}")
                } else {
                    write!(output, "\\U{point:08x}")
                }
                .map_err(|_| Error::value("text formatting failed"))?;
            }
        }
    }
    output.push(quote);
    Ok(output)
}

pub(super) fn value(value: &Value) -> Result<String, Error> {
    match value {
        Value::Text(points) => text(points),
        Value::Array(values) => Ok(format!(
            "[{}]",
            values
                .iter()
                .map(Value::repr)
                .collect::<Result<Vec<_>, _>>()?
                .join(", ")
        )),
        Value::Object(fields) => Ok(format!(
            "{{{}}}",
            fields
                .iter()
                .map(|(key, value)| Ok(format!("{}: {}", text(key)?, value.repr()?)))
                .collect::<Result<Vec<_>, Error>>()?
                .join(", ")
        )),
        Value::Null | Value::Bool(_) | Value::Integer(_) | Value::Float(_) => value.string(),
    }
}
