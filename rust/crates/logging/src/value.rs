use indexmap::IndexMap;
use std::{
    fmt::{self, Write},
    ops::{Deref, DerefMut},
};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Fields(IndexMap<String, Value>);

impl Deref for Fields {
    type Target = IndexMap<String, Value>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for Fields {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl FromIterator<(String, Value)> for Fields {
    fn from_iter<T: IntoIterator<Item = (String, Value)>>(values: T) -> Self {
        Self(values.into_iter().collect())
    }
}
impl Fields {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn to_json(&self) -> Result<String, fmt::Error> {
        let mut output = String::new();
        self.write(&mut output)?;
        Ok(output)
    }
    fn write(&self, output: &mut String) -> fmt::Result {
        output.push('{');
        for (index, (key, value)) in self.0.iter().enumerate() {
            if index != 0 {
                output.push_str(", ");
            }
            quoted(key.chars().map(u32::from), output)?;
            output.push_str(": ");
            value.write(output)?;
        }
        output.push('}');
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Number {
    Integer(i64),
    Float(f64),
}
impl Number {
    pub fn greater_than(self, other: Self) -> bool {
        fn compare(integer: i64, float: f64) -> Option<std::cmp::Ordering> {
            if float.is_nan() {
                return None;
            }
            if float < i64::MIN as f64 {
                return Some(std::cmp::Ordering::Greater);
            }
            if float >= -(i64::MIN as f64) {
                return Some(std::cmp::Ordering::Less);
            }
            let truncated = float as i64;
            match integer.cmp(&truncated) {
                std::cmp::Ordering::Equal => (truncated as f64).partial_cmp(&float),
                ordering => Some(ordering),
            }
        }
        match (self, other) {
            (Self::Integer(left), Self::Integer(right)) => left > right,
            (Self::Float(left), Self::Float(right)) => left > right,
            (Self::Integer(left), Self::Float(right)) => {
                compare(left, right) == Some(std::cmp::Ordering::Greater)
            }
            (Self::Float(left), Self::Integer(right)) => {
                compare(right, left) == Some(std::cmp::Ordering::Less)
            }
        }
    }
    pub fn as_float(self) -> f64 {
        match self {
            Self::Integer(value) => value as f64,
            Self::Float(value) => value,
        }
    }
    pub fn rounded(self) -> Result<Value, std::num::ParseFloatError> {
        match self {
            Self::Integer(value) => Ok(Value::Integer(i128::from(value))),
            Self::Float(value) => Ok(Value::Float(round3(value)?)),
        }
    }
}

pub(crate) fn round3(value: f64) -> Result<f64, std::num::ParseFloatError> {
    format!("{value:.3}").parse()
}

/// Python strings may contain lone surrogates, including Unix surrogateescape filenames.
#[derive(Clone, Debug, PartialEq)]
pub struct PythonText(Vec<u32>);
impl PythonText {
    pub fn new(points: Vec<u32>) -> Result<Self, crate::Error> {
        if points.iter().any(|point| *point > 0x10ffff) {
            return Err(crate::Error::Contract(
                "Python text codepoint exceeds Unicode range",
            ));
        }
        Ok(Self(points))
    }
    pub fn codepoints(&self) -> &[u32] {
        &self.0
    }
    pub fn console(&self) -> String {
        let mut text = String::new();
        for point in &self.0 {
            if let Some(character) = char::from_u32(*point) {
                text.push(character);
            } else {
                let _ = write!(text, "\\u{point:04x}");
            }
        }
        text
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Integer(i128),
    Float(f64),
    Text(String),
    PythonText(PythonText),
    Array(Vec<Value>),
    Object(Fields),
}
impl Value {
    pub fn from_json(value: serde_json::Value) -> Result<Self, crate::Error> {
        Ok(match value {
            serde_json::Value::Null => Self::Null,
            serde_json::Value::Bool(value) => Self::Bool(value),
            serde_json::Value::String(value) => Self::Text(value),
            serde_json::Value::Array(values) => Self::Array(
                values
                    .into_iter()
                    .map(Self::from_json)
                    .collect::<Result<_, _>>()?,
            ),
            serde_json::Value::Object(values) => Self::Object(
                values
                    .into_iter()
                    .map(|(key, value)| Ok((key, Self::from_json(value)?)))
                    .collect::<Result<_, crate::Error>>()?,
            ),
            serde_json::Value::Number(value) => {
                if let Some(value) = value.as_i64() {
                    Self::Integer(i128::from(value))
                } else if let Some(value) = value.as_u64() {
                    Self::Integer(i128::from(value))
                } else {
                    Self::Float(value.as_f64().ok_or(crate::Error::Contract(
                        "JSON number has no numeric representation",
                    ))?)
                }
            }
        })
    }
    fn write(&self, output: &mut String) -> fmt::Result {
        match self {
            Self::Null => output.push_str("null"),
            Self::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
            Self::Integer(value) => write!(output, "{value}")?,
            Self::Float(value) => {
                openpilot_runtime_core::python_float::write_float(*value, output)?
            }
            Self::Text(value) => quoted(value.chars().map(u32::from), output)?,
            Self::PythonText(value) => quoted(value.0.iter().copied(), output)?,
            Self::Object(value) => value.write(output)?,
            Self::Array(values) => {
                output.push('[');
                for (index, value) in values.iter().enumerate() {
                    if index != 0 {
                        output.push_str(", ");
                    }
                    value.write(output)?;
                }
                output.push(']');
            }
        }
        Ok(())
    }
}

fn quoted(points: impl IntoIterator<Item = u32>, output: &mut String) -> fmt::Result {
    output.push('"');
    for point in points {
        match point {
            8 => output.push_str("\\b"),
            9 => output.push_str("\\t"),
            10 => output.push_str("\\n"),
            12 => output.push_str("\\f"),
            13 => output.push_str("\\r"),
            34 => output.push_str("\\\""),
            92 => output.push_str("\\\\"),
            32..=126 => {
                if let Some(character) = char::from_u32(point) {
                    output.push(character);
                }
            }
            0..=0xffff => write!(output, "\\u{point:04x}")?,
            _ => {
                let value = point - 0x10000;
                write!(
                    output,
                    "\\u{:04x}\\u{:04x}",
                    0xd800 + (value >> 10),
                    0xdc00 + (value & 0x3ff)
                )?;
            }
        }
    }
    output.push('"');
    Ok(())
}
