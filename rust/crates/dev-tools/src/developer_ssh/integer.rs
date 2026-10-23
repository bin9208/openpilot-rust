//! Bounded Python3.12 decimal string-int policy for developer SSH port arguments.
//! Copyright (c) 2018, Comma.ai, Inc.; repository MIT license applies.
//! Constants/ordering inspected in pinned CPython3.12.15 headers and static code.
use crate::argparse_scan::decimal_value;
use num_bigint::BigInt;
use std::{ffi::OsStr, fmt, os::unix::ffi::OsStrExt};

pub const DEFAULT_MAX_STR_DIGITS: u32 = 4300;
pub const STR_DIGITS_CHECK_THRESHOLD: u32 = 640;

/// Explicit conversion policy, not a process-global or dynamically read environment setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntStringLimit(u32);
impl Default for IntStringLimit {
    fn default() -> Self {
        Self(DEFAULT_MAX_STR_DIGITS)
    }
}
impl IntStringLimit {
    pub fn max_digits(self) -> u32 {
        self.0
    }
    pub fn from_max_digits(value: u32) -> Result<Self, StartupLimitError> {
        if value == 0 || (STR_DIGITS_CHECK_THRESHOLD..=i32::MAX as u32).contains(&value) {
            Ok(Self(value))
        } else {
            Err(StartupLimitError)
        }
    }
    /// Pure model of the default Linux CPython startup environment conversion.
    /// The CLI passes its snapshot once; direct library parsing never reads env.
    pub fn from_startup_env(value: Option<&OsStr>) -> Result<Self, StartupLimitError> {
        let Some(value) = value.filter(|value| !value.is_empty()) else {
            return Ok(Self::default());
        };
        // CPython uses strtol(base=10), then checks signed C-int range and 0/>=640.
        // Leading C whitespace/sign are allowed, but no trailing chars/whitespace,
        // underscores or Unicode Nd; this is NOT Python's string-int grammar.
        let bytes = value.as_bytes();
        // C isspace includes vertical tab; Rust's ASCII whitespace helper differs.
        let leading = bytes
            .iter()
            .take_while(|&&byte| matches!(byte, b'\t'..=b'\r' | b' '))
            .count();
        let bytes = &bytes[leading..];
        let (negative, digits) = match bytes.first() {
            Some(b'-') => (true, &bytes[1..]),
            Some(b'+') => (false, &bytes[1..]),
            _ => (false, bytes),
        };
        if digits.is_empty() {
            return Err(StartupLimitError);
        }
        let mut number = 0u32;
        for &digit in digits {
            if !digit.is_ascii_digit() {
                return Err(StartupLimitError);
            }
            number = number
                .checked_mul(10)
                .and_then(|value| value.checked_add(u32::from(digit - b'0')))
                .filter(|&value| value <= i32::MAX as u32)
                .ok_or(StartupLimitError)?;
        }
        if negative && number != 0 {
            return Err(StartupLimitError);
        }
        Self::from_max_digits(number)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartupLimitError;
impl fmt::Display for StartupLimitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PYTHONINTMAXSTRDIGITS: invalid limit; must be >= 640 or 0 for unlimited.")
    }
}
impl std::error::Error for StartupLimitError {}

#[derive(Debug, PartialEq, Eq)]
pub enum DecimalError {
    InvalidLiteral(String),
    DigitLimit { limit: u32, digits: usize },
}
impl fmt::Display for DecimalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLiteral(input) => write!(f, "invalid literal for int() with base 10: {input:?}"),
            Self::DigitLimit { limit, digits } => write!(f,
                "Exceeds the limit ({limit} digits) for integer string conversion: value has {digits} digits; use sys.set_int_max_str_digits() to increase the limit"),
        }
    }
}
impl std::error::Error for DecimalError {}

fn integer_space(ch: char) -> bool {
    // CPython's ASCII C whitespace plus Unicode15 non-ASCII whitespace mapped to
    // ASCII by TransformDecimalAndSpaceToASCII. U+001C..1F remain invalid ASCII.
    matches!(ch, '\t'..='\r' | ' ' | '\u{85}' | '\u{a0}' | '\u{1680}' |
        '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' |
        '\u{205f}' | '\u{3000}')
}
/// Parse int(text,10), then its decimal output; enforce the input guard BEFORE BigInt.
/// Leading zeros count. Signs, surrounding whitespace and separators do not count.
/// This pure subset accepts strings only and is not a general Python int replacement.
pub fn parse_decimal(input: &str, limit: IntStringLimit) -> Result<String, DecimalError> {
    let invalid = || DecimalError::InvalidLiteral(input.to_owned());
    let normalized = input
        .trim_matches(integer_space)
        .chars()
        .map(|ch| decimal_value(ch).map_or(ch, |digit| char::from(b'0' + digit)))
        .collect::<String>();
    let digits = normalized.strip_prefix(['+', '-']).unwrap_or(&normalized);
    // CPython scans the whole token/underscore grammar before checking the count.
    if digits.is_empty()
        || !digits
            .split('_')
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err(invalid());
    }
    let count = digits.bytes().filter(u8::is_ascii_digit).count();
    if limit.0 != 0 && count > limit.0 as usize {
        return Err(DecimalError::DigitLimit {
            limit: limit.0,
            digits: count,
        });
    }
    normalized
        .replace('_', "")
        .parse::<BigInt>()
        .map(|value| value.to_string())
        .map_err(|_| invalid())
}
