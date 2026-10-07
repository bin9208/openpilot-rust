use crate::{json::Value, Error};
use num_bigint::BigInt;

const DECIMAL_ZEROES: &[u32] = &[
    48, 1632, 1776, 1984, 2406, 2534, 2662, 2790, 2918, 3046, 3174, 3302, 3430, 3558, 3664, 3792,
    3872, 4160, 4240, 6112, 6160, 6470, 6608, 6784, 6800, 6992, 7088, 7232, 7248, 42528, 43216,
    43264, 43472, 43504, 43600, 44016, 65296, 66720, 68912, 69734, 69872, 69942, 70096, 70384,
    70736, 70864, 71248, 71360, 71472, 71904, 72016, 72784, 73040, 73120, 73552, 92768, 92864,
    93008, 120782, 120792, 120802, 120812, 120822, 123200, 123632, 124144, 125264, 130032,
];

pub(crate) fn parse(points: &[u32]) -> Result<BigInt, Error> {
    let error = || {
        Error::value(&format!(
            "invalid literal for int() with base 10: {}",
            Value::Text(points.to_vec()).repr().unwrap_or_default()
        ))
    };
    let raw: String = points
        .iter()
        .copied()
        .map(char::from_u32)
        .collect::<Option<_>>()
        .ok_or_else(error)?;
    let raw = raw.trim_matches(char::is_whitespace);
    let (negative, raw) = if let Some(raw) = raw.strip_prefix('-') {
        (true, raw)
    } else {
        (false, raw.strip_prefix('+').unwrap_or(raw))
    };
    let mut digits = String::with_capacity(raw.len());
    let mut previous_digit = false;
    for point in raw.chars().map(u32::from) {
        if point == u32::from('_') && previous_digit {
            previous_digit = false;
            continue;
        }
        let digit = DECIMAL_ZEROES
            .iter()
            .find_map(|&zero| point.checked_sub(zero).filter(|&digit| digit < 10));
        let digit = digit
            .and_then(|digit| char::from_u32(u32::from('0') + digit))
            .ok_or_else(error)?;
        digits.push(digit);
        previous_digit = true;
    }
    if !previous_digit {
        return Err(error());
    }
    if digits.len() > 4300 {
        return Err(Error::value(
            "Exceeds the limit (4300 digits) for integer string conversion",
        ));
    }
    let number = BigInt::parse_bytes(digits.as_bytes(), 10).ok_or_else(error)?;
    Ok(if negative { -number } else { number })
}
