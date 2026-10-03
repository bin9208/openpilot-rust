use crate::core::Error;
use num_bigint::BigInt;
use num_traits::ToPrimitive;

const DECIMAL_ZEROES: &[u32] = &[
    0x30, 0x660, 0x6f0, 0x7c0, 0x966, 0x9e6, 0xa66, 0xae6, 0xb66, 0xbe6, 0xc66, 0xce6, 0xd66,
    0xde6, 0xe50, 0xed0, 0xf20, 0x1040, 0x1090, 0x17e0, 0x1810, 0x1946, 0x19d0, 0x1a80, 0x1a90,
    0x1b50, 0x1bb0, 0x1c40, 0x1c50, 0xa620, 0xa8d0, 0xa900, 0xa9d0, 0xa9f0, 0xaa50, 0xabf0, 0xff10,
    0x104a0, 0x10d30, 0x11066, 0x110f0, 0x11136, 0x111d0, 0x112f0, 0x11450, 0x114d0, 0x11650,
    0x116c0, 0x11730, 0x118e0, 0x11950, 0x11c50, 0x11d50, 0x11da0, 0x11f50, 0x16a60, 0x16ac0,
    0x16b50, 0x1d7ce, 0x1d7d8, 0x1d7e2, 0x1d7ec, 0x1d7f6, 0x1e140, 0x1e2f0, 0x1e4f0, 0x1e950,
    0x1fbf0,
];

pub(super) fn command_speed(text: &str) -> Result<Option<f64>, Error> {
    let text = text.trim();
    let (negative, digits) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let mut normalized = String::with_capacity(digits.len());
    let mut previous = false;
    for c in digits.chars() {
        if c == '_' {
            if !previous {
                return Err(Error::Numeric);
            }
            previous = false;
        } else {
            let point = u32::from(c);
            let digit = DECIMAL_ZEROES
                .iter()
                .find_map(|zero| point.checked_sub(*zero).filter(|&d| d < 10))
                .ok_or(Error::Numeric)?;
            normalized.push(char::from_u32(u32::from('0') + digit).ok_or(Error::Numeric)?);
            previous = true;
        }
    }
    if !previous || normalized.len() > 4300 {
        return Err(Error::Numeric);
    }
    let integer = BigInt::parse_bytes(normalized.as_bytes(), 10).ok_or(Error::Numeric)?;
    if negative {
        return Ok(None);
    }
    Ok(integer
        .to_i32()
        .filter(|&value| 0 < value && value < 200)
        .map(f64::from))
}

#[cfg(test)]
mod tests {
    use super::command_speed;

    #[test]
    fn malformed_speed_literals_retain_source_failure() {
        for input in ["", "1__0", "_99", "99_", "1.0", "NaN", "\u{1c}99"] {
            assert!(command_speed(input).is_err(), "{input:?}");
        }
    }

    #[test]
    fn oversized_decimal_literal_retains_python_digit_limit() {
        let input = "1".repeat(4301);
        assert!(command_speed(&input).is_err());
    }
}
