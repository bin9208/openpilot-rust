use crate::{Error, Fields, Value};

pub(crate) fn truth(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Integer(value) => *value != 0,
        Value::Float(value) => *value != 0.,
        Value::Text(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
    }
}
pub(crate) fn text(value: &Value) -> Result<String, Error> {
    Ok(match value {
        Value::Null => "None".into(),
        Value::Bool(value) => if *value { "True" } else { "False" }.into(),
        Value::Integer(value) => value.to_string(),
        Value::Float(value) => {
            let mut output = String::new();
            openpilot_runtime_core::python_float::write_float(*value, &mut output)?;
            output
        }
        Value::Text(value) => value.clone(),
        Value::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(repr)
                .collect::<Result<Vec<_>, _>>()?
                .join(", ")
        ),
        Value::Object(values) => format!(
            "{{{}}}",
            values
                .iter()
                .map(|(key, value)| Ok(format!(
                    "{}: {}",
                    repr(&Value::Text(key.clone()))?,
                    repr(value)?
                )))
                .collect::<Result<Vec<_>, Error>>()?
                .join(", ")
        ),
    })
}
fn repr(value: &Value) -> Result<String, Error> {
    if let Value::Text(value) = value {
        let quote = if value.contains('\'') && !value.contains('"') {
            '"'
        } else {
            '\''
        };
        let mut output = String::new();
        output.push(quote);
        for character in value.chars() {
            match character {
                '\\' => output.push_str("\\\\"),
                '\n' => output.push_str("\\n"),
                '\r' => output.push_str("\\r"),
                '\t' => output.push_str("\\t"),
                ch if ch == quote => {
                    output.push('\\');
                    output.push(ch);
                }
                ch if ch.is_control() || ch.escape_debug().to_string().starts_with("\\u{") => {
                    use std::fmt::Write;
                    let point = u32::from(ch);
                    if point <= 0xff {
                        write!(output, "\\x{point:02x}")?;
                    } else if point <= 0xffff {
                        write!(output, "\\u{point:04x}")?;
                    } else {
                        write!(output, "\\U{point:08x}")?;
                    }
                }
                ch => output.push(ch),
            }
        }
        output.push(quote);
        Ok(output)
    } else {
        text(value)
    }
}
pub(crate) fn or_empty(value: Option<&Value>) -> Result<String, Error> {
    match value.filter(|value| truth(value)) {
        Some(value) => text(value),
        None => Ok(String::new()),
    }
}
pub(crate) fn truncate(value: &str, count: usize) -> String {
    value.chars().take(count).collect()
}
pub(crate) fn body_mapping(value: Value) -> Result<Fields, Error> {
    if !truth(&value) {
        return Ok(Fields::new());
    }
    match value {
        Value::Object(fields) => Ok(fields),
        Value::Array(_) => Err(Error::BodyShape("list")),
        Value::Text(_) => Err(Error::BodyShape("str")),
        Value::Integer(_) => Err(Error::BodyShape("int")),
        Value::Float(_) => Err(Error::BodyShape("float")),
        Value::Bool(_) => Err(Error::BodyShape("bool")),
        Value::Null => Ok(Fields::new()),
    }
}
pub(crate) fn remote_size(value: Option<&Value>) -> Result<i128, Error> {
    match value {
        None | Some(Value::Null) => Ok(-1),
        Some(Value::Integer(value)) => Ok(*value),
        Some(Value::Bool(value)) => Ok(i128::from(*value)),
        Some(Value::Float(value)) if value.is_finite() => format!("{:.0}", value.trunc())
            .parse()
            .map_err(|_| Error::Source("remote size exceeds integer range".into())),
        Some(Value::Text(value)) => parse_integer(value),
        Some(Value::Array(_)) => Err(Error::Source(
            "int() argument must be a string, a bytes-like object or a real number, not 'list'"
                .into(),
        )),
        Some(Value::Object(_)) => Err(Error::Source(
            "int() argument must be a string, a bytes-like object or a real number, not 'dict'"
                .into(),
        )),
        Some(Value::Float(_)) => Err(Error::Source(
            "cannot convert non-finite float to integer".into(),
        )),
    }
}

pub(crate) fn strip(value: &str) -> &str {
    value.trim_matches(|ch: char| ch.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&ch))
}
fn parse_integer(value: &str) -> Result<i128, Error> {
    let raw = value.trim();
    let (sign, digits) = if let Some(rest) = raw.strip_prefix('-') {
        ("-", rest)
    } else {
        ("", raw.strip_prefix('+').unwrap_or(raw))
    };
    let mut output = String::from(sign);
    let mut previous_digit = false;
    let mut valid = !digits.is_empty();
    for point in digits.chars().map(u32::from) {
        if point == u32::from('_') && previous_digit {
            previous_digit = false;
            continue;
        }
        let digit = DECIMAL_ZEROES
            .iter()
            .find_map(|zero| point.checked_sub(*zero).filter(|digit| *digit < 10));
        match digit.and_then(|digit| char::from_u32(u32::from('0') + digit)) {
            Some(digit) => {
                output.push(digit);
                previous_digit = true;
            }
            None => {
                valid = false;
                break;
            }
        }
    }
    if valid && previous_digit {
        if let Ok(number) = output.parse() {
            return Ok(number);
        }
    }
    Err(Error::Source(format!(
        "invalid literal for int() with base 10: {}",
        repr(&Value::Text(value.into()))?
    )))
}
// Unicode 15.0 decimal zeroes, matching the source's pinned Python 3.12 runtime.
const DECIMAL_ZEROES: &[u32] = &[
    0x30, 0x660, 0x6f0, 0x7c0, 0x966, 0x9e6, 0xa66, 0xae6, 0xb66, 0xbe6, 0xc66, 0xce6, 0xd66,
    0xde6, 0xe50, 0xed0, 0xf20, 0x1040, 0x1090, 0x17e0, 0x1810, 0x1946, 0x19d0, 0x1a80, 0x1a90,
    0x1b50, 0x1bb0, 0x1c40, 0x1c50, 0xa620, 0xa8d0, 0xa900, 0xa9d0, 0xa9f0, 0xaa50, 0xabf0, 0xff10,
    0x104a0, 0x10d30, 0x11066, 0x110f0, 0x11136, 0x111d0, 0x112f0, 0x11450, 0x114d0, 0x11650,
    0x116c0, 0x11730, 0x118e0, 0x11950, 0x11c50, 0x11d50, 0x11da0, 0x11f50, 0x16a60, 0x16ac0,
    0x16b50, 0x1d7ce, 0x1d7d8, 0x1d7e2, 0x1d7ec, 0x1d7f6, 0x1e140, 0x1e2f0, 0x1e4f0, 0x1e950,
    0x1fbf0,
];
