use crate::Error;
use num_bigint::BigInt;
use num_traits::{FromPrimitive, ToPrimitive, Zero};
use openpilot_logmessaged::{JsonValue, JsonView};
use std::fmt::Write;
mod errors;
mod repr;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Integer(BigInt),
    Float(f64),
    Text(Vec<u32>),
    Array(Vec<Self>),
    Object(Vec<(Vec<u32>, Self)>),
}

impl Value {
    pub fn parse(text: &str) -> Result<Self, Error> {
        let parsed = JsonValue::parse(text).map_err(|error| errors::syntax(text, error))?;
        Self::from_view(&parsed)
    }

    fn from_view(value: &JsonValue) -> Result<Self, Error> {
        Ok(match value.view() {
            JsonView::Null => Self::Null,
            JsonView::Bool(value) => Self::Bool(value),
            JsonView::Integer(value) => Self::Integer(
                BigInt::parse_bytes(value.as_bytes(), 10)
                    .ok_or_else(|| Error::value("invalid parsed integer"))?,
            ),
            JsonView::Float(value) => Self::Float(value),
            JsonView::Text(value) => Self::Text(value.to_vec()),
            JsonView::Array(values) => Self::Array(
                values
                    .iter()
                    .map(Self::from_view)
                    .collect::<Result<_, _>>()?,
            ),
            JsonView::Object(values) => Self::Object(
                values
                    .into_iter()
                    .map(|(key, value)| Ok((key.to_vec(), Self::from_view(&value)?)))
                    .collect::<Result<_, Error>>()?,
            ),
        })
    }

    pub fn object<const N: usize>(fields: [(&str, Self); N]) -> Self {
        Self::Object(
            fields
                .into_iter()
                .map(|(key, value)| (key.chars().map(u32::from).collect(), value))
                .collect(),
        )
    }

    pub fn text(text: &str) -> Self {
        Self::Text(text.chars().map(u32::from).collect())
    }
    pub fn integer(value: impl Into<BigInt>) -> Self {
        Self::Integer(value.into())
    }
    pub fn get(&self, key: &str) -> &Self {
        if let Self::Object(fields) = self {
            if let Some((_, value)) = fields
                .iter()
                .find(|(name, _)| name.iter().copied().eq(key.chars().map(u32::from)))
            {
                return value;
            }
        }
        &Self::Null
    }
    pub fn has(&self, key: &str) -> bool {
        matches!(self, Self::Object(fields) if fields.iter().any(|(name, _)| name.iter().copied().eq(key.chars().map(u32::from))))
    }
    pub fn text_eq(&self, text: &str) -> bool {
        matches!(self, Self::Text(points) if points.iter().copied().eq(text.chars().map(u32::from)))
    }
    pub fn number_eq(&self, number: i64) -> bool {
        match self {
            Self::Integer(value) => *value == BigInt::from(number),
            Self::Float(value) => BigInt::from_f64(*value)
                .is_some_and(|integer| integer == BigInt::from(number) && value.fract() == 0.),
            Self::Bool(value) => i64::from(*value) == number,
            Self::Null | Self::Text(_) | Self::Array(_) | Self::Object(_) => false,
        }
    }
    pub fn truth(&self) -> bool {
        match self {
            Self::Null => false,
            Self::Bool(value) => *value,
            Self::Integer(value) => !value.is_zero(),
            Self::Float(value) => *value != 0.,
            Self::Text(value) => !value.is_empty(),
            Self::Array(value) => !value.is_empty(),
            Self::Object(value) => !value.is_empty(),
        }
    }
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Null => "NoneType",
            Self::Bool(_) => "bool",
            Self::Integer(_) => "int",
            Self::Float(_) => "float",
            Self::Text(_) => "str",
            Self::Array(_) => "list",
            Self::Object(_) => "dict",
        }
    }
    pub fn int(&self) -> Result<BigInt, Error> {
        match self {
            Self::Integer(value) => Ok(value.clone()),
            Self::Bool(value) => Ok(BigInt::from(u8::from(*value))),
            Self::Float(value) if value.is_nan() => Err(Error::value("cannot convert float NaN to integer")),
            Self::Float(value) => BigInt::from_f64(*value).ok_or_else(|| Error::typed("OverflowError", "cannot convert float infinity to integer".into())),
            Self::Text(points) => super::integer::parse(points),
            Self::Null | Self::Array(_) | Self::Object(_) => Err(Error::typed("TypeError", format!(
                "int() argument must be a string, a bytes-like object or a real number, not '{}'", self.type_name()))),
        }
    }
    pub fn float(&self) -> Result<f64, Error> {
        match self {
            Self::Float(value) => Ok(*value),
            Self::Bool(value) => Ok(f64::from(u8::from(*value))),
            Self::Integer(value) => {
                value
                    .to_f64()
                    .filter(|value| value.is_finite())
                    .ok_or_else(|| {
                        Error::typed("OverflowError", "int too large to convert to float".into())
                    })
            }
            Self::Text(points) => {
                let text = points
                    .iter()
                    .copied()
                    .map(char::from_u32)
                    .collect::<Option<String>>();
                text.as_deref()
                    .and_then(openpilot_runtime_core::python_float::parse)
                    .ok_or_else(|| Error::value("could not convert string to float"))
            }
            Self::Null | Self::Array(_) | Self::Object(_) => Err(Error::typed(
                "TypeError",
                format!(
                    "float() argument must be a string or a real number, not '{}'",
                    self.type_name()
                ),
            )),
        }
    }
    pub fn string(&self) -> Result<String, Error> {
        Ok(match self {
            Self::Text(points) => points
                .iter()
                .copied()
                .map(char::from_u32)
                .collect::<Option<String>>()
                .ok_or_else(|| {
                    Error::typed(
                        "UnicodeEncodeError",
                        "cannot encode lone surrogate as UTF-8".into(),
                    )
                })?,
            Self::Null => "None".into(),
            Self::Bool(value) => if *value { "True" } else { "False" }.into(),
            Self::Integer(value) => value.to_string(),
            Self::Float(value) if value.is_nan() => "nan".into(),
            Self::Float(value) if value.is_infinite() => if value.is_sign_negative() {
                "-inf"
            } else {
                "inf"
            }
            .into(),
            Self::Float(value) => {
                let mut text = String::new();
                openpilot_runtime_core::python_float::write_float(*value, &mut text)
                    .map_err(|_| Error::value("float formatting failed"))?;
                text
            }
            Self::Array(_) | Self::Object(_) => self.repr()?,
        })
    }
    pub fn py_string(&self) -> Result<Self, Error> {
        match self {
            Self::Text(points) => Ok(Self::Text(points.clone())),
            Self::Null
            | Self::Bool(_)
            | Self::Integer(_)
            | Self::Float(_)
            | Self::Array(_)
            | Self::Object(_) => Ok(Self::text(&self.string()?)),
        }
    }
    pub fn repr(&self) -> Result<String, Error> {
        repr::value(self)
    }
    pub fn encode(&self) -> Result<String, Error> {
        let mut output = String::new();
        self.write(&mut output)?;
        Ok(output)
    }
    fn write(&self, output: &mut String) -> Result<(), Error> {
        match self {
            Self::Null => output.push_str("null"),
            Self::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
            Self::Integer(value) => {
                write!(output, "{value}").map_err(|_| Error::value("integer formatting failed"))?
            }
            Self::Float(value) => openpilot_runtime_core::python_float::write_float(*value, output)
                .map_err(|_| Error::value("float formatting failed"))?,
            Self::Text(points) => output.push_str(
                &JsonValue::codepoints(points.clone())
                    .ok_or_else(|| Error::value("invalid Python text"))?
                    .to_json()
                    .map_err(|_| Error::value("text formatting failed"))?,
            ),
            Self::Array(values) => {
                output.push('[');
                for (index, value) in values.iter().enumerate() {
                    if index > 0 {
                        output.push_str(", ");
                    }
                    value.write(output)?;
                }
                output.push(']');
            }
            Self::Object(fields) => {
                output.push('{');
                for (index, (key, value)) in fields.iter().enumerate() {
                    if index > 0 {
                        output.push_str(", ");
                    }
                    Self::Text(key.clone()).write(output)?;
                    output.push_str(": ");
                    value.write(output)?;
                }
                output.push('}');
            }
        }
        Ok(())
    }
}
