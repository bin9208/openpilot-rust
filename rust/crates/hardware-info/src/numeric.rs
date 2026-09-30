use crate::{Error, JsonValue};
use num_bigint::BigInt;
use num_traits::{FromPrimitive, ToPrimitive, Zero};

#[derive(Debug, Clone, PartialEq)]
pub enum Number {
    Integer(BigInt),
    Float(f64),
}
impl Number {
    pub fn zero() -> Self {
        Self::Integer(BigInt::zero())
    }
    pub fn to_f64(&self) -> Result<f64, Error> {
        match self {
            Self::Integer(value) => integer_float(value),
            Self::Float(value) => Ok(*value),
        }
    }
    pub fn to_json(&self) -> Result<JsonValue, Error> {
        let text = match self {
            Self::Integer(value) => value.to_string(),
            Self::Float(value) => {
                let mut text = String::new();
                openpilot_runtime_core::python_float::write_float(*value, &mut text)?;
                text
            }
        };
        Ok(JsonValue::parse(&text)?)
    }
}
pub(crate) fn whitespace(character: char) -> bool {
    character.is_whitespace() || matches!(character, '\x1c'..='\x1f')
}
pub(crate) fn trim(text: &str) -> &str {
    text.trim_matches(whitespace)
}
fn ascii_space(character: char) -> bool {
    matches!(character, ' ' | '\t' | '\r' | '\n' | '\x0b' | '\x0c')
}
include!("decimal_digits.rs");
fn normalize(text: &str) -> Result<String, Error> {
    let mut result = String::with_capacity(text.len());
    for character in text.chars() {
        if character.is_ascii() {
            result.push(character);
        } else if whitespace(character) {
            result.push(' ');
        } else if let Some(digit) = decimal_digit(character) {
            result.push(char::from(b'0' + digit));
        } else {
            return Err(Error::Value("invalid numeric character"));
        }
    }
    Ok(result.trim_matches(ascii_space).to_owned())
}
pub(crate) fn integer(text: &str, radix: u32) -> Result<BigInt, Error> {
    let text = normalize(text)?;
    let (negative, unsigned) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text.as_str()),
    };
    let mut unsigned = unsigned;
    if radix == 16 && (unsigned.starts_with("0x") || unsigned.starts_with("0X")) {
        unsigned = &unsigned[2..];
        if let Some(rest) = unsigned.strip_prefix('_') {
            unsigned = rest;
        }
    }
    let mut digits = String::new();
    let mut previous_digit = false;
    for character in unsigned.chars() {
        if character == '_' {
            if !previous_digit {
                return Err(Error::Value("invalid integer underscore"));
            }
            previous_digit = false;
        } else if character.is_digit(radix) {
            digits.push(character);
            previous_digit = true;
        } else {
            return Err(Error::Value("invalid integer literal"));
        }
    }
    if !previous_digit {
        return Err(Error::Value("empty or incomplete integer"));
    }
    if radix == 10 && digits.len() > 4300 {
        return Err(Error::Value("integer exceeds Python 4300-digit limit"));
    }
    let value =
        BigInt::parse_bytes(digits.as_bytes(), radix).ok_or(Error::Value("invalid integer"))?;
    Ok(if negative { -value } else { value })
}
pub(crate) fn float(text: &str) -> Result<f64, Error> {
    let text = normalize(text)?;
    let bytes = text.as_bytes();
    let mut clean = String::with_capacity(text.len());
    for (index, character) in text.char_indices() {
        if character == '_' {
            if index == 0
                || !bytes[index - 1].is_ascii_digit()
                || !bytes.get(index + 1).is_some_and(u8::is_ascii_digit)
            {
                return Err(Error::Value("invalid float underscore"));
            }
        } else {
            clean.push(character);
        }
    }
    match clean.to_ascii_lowercase().as_str() {
        "nan" | "+nan" => return Ok(f64::NAN),
        "-nan" => return Ok(-f64::NAN),
        "inf" | "+inf" | "infinity" | "+infinity" => return Ok(f64::INFINITY),
        "-inf" | "-infinity" => return Ok(f64::NEG_INFINITY),
        _ => {}
    }
    if !clean.bytes().any(|byte| byte.is_ascii_digit())
        || clean
            .bytes()
            .any(|byte| !matches!(byte, b'0'..=b'9' | b'+' | b'-' | b'.' | b'e' | b'E'))
    {
        return Err(Error::Value("invalid float literal"));
    }
    clean
        .parse()
        .map_err(|_| Error::Value("invalid float literal"))
}
pub(crate) fn integer_float(value: &BigInt) -> Result<f64, Error> {
    value
        .to_f64()
        .filter(|value| value.is_finite())
        .ok_or(Error::Overflow)
}
pub(crate) fn float_integer(value: f64) -> Result<BigInt, Error> {
    if value.is_nan() {
        return Err(Error::Value("cannot convert NaN to integer"));
    }
    BigInt::from_f64(value).ok_or(Error::Overflow)
}
pub(crate) fn divide(value: &BigInt, divisor: f64) -> Result<f64, Error> {
    let value = integer_float(value)?;
    if divisor == 0.0 {
        return Err(Error::ZeroDivision);
    }
    Ok(value / divisor)
}
