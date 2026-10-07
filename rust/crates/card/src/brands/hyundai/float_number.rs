use super::Error;
use num_bigint::BigUint;
use num_traits::{ToPrimitive, Zero};

pub fn nan(raw: &str, sign: u32) -> f32 {
    let payload = raw
        .strip_prefix('(')
        .and_then(|raw| raw.split_once(')').map(|(payload, _)| payload));
    let bits = payload
        .and_then(|payload| {
            let (digits, radix) = if let Some(hex) = payload
                .strip_prefix("0x")
                .or_else(|| payload.strip_prefix("0X"))
            {
                (hex, 16)
            } else if payload.starts_with('0') {
                (payload, 8)
            } else {
                (payload, 10)
            };
            BigUint::parse_bytes(digits.as_bytes(), radix)
                .map(|value| (&value & BigUint::from(0x7f_ffffu32)).to_u32().unwrap_or(0))
        })
        .unwrap_or(0);
    f32::from_bits(sign | 0x7fc0_0000 | bits)
}

fn exponent(raw: &str) -> i64 {
    let negative = raw.starts_with('-');
    let raw = raw.trim_start_matches(['+', '-']);
    let value = raw
        .bytes()
        .take_while(u8::is_ascii_digit)
        .fold(0i64, |value, byte| {
            (value * 10 + i64::from(byte - b'0')).min(10000)
        });
    if negative {
        -value
    } else {
        value
    }
}

fn equal_float(numerator: &BigUint, denominator: &BigUint, value: f32) -> Result<bool, Error> {
    let bits = value.to_bits() & 0x7fff_ffff;
    let encoded = (bits >> 23) & 255;
    let significand = BigUint::from((bits & 0x7f_ffff) | if encoded == 0 { 0 } else { 0x80_0000 });
    let power = if encoded == 0 {
        -149
    } else {
        i64::from(encoded) - 150
    };
    if power < 0 {
        Ok(
            numerator << usize::try_from(-power).map_err(|_| Error::Numeric)?
                == significand * denominator,
        )
    } else {
        Ok(*numerator
            == (significand << usize::try_from(power).map_err(|_| Error::Numeric)?) * denominator)
    }
}

fn range(numerator: &BigUint, denominator: &BigUint, value: f32, key: &str) -> Result<(), Error> {
    if !value.is_finite() || (value == 0. && !numerator.is_zero()) {
        return Err(Error::FloatRange(key.into()));
    }
    let magnitude = value.abs();
    if magnitude < f32::MIN_POSITIVE && !equal_float(numerator, denominator, magnitude)? {
        return Err(Error::FloatRange(key.into()));
    }
    if magnitude == f32::MIN_POSITIVE
        && numerator << 150usize <= BigUint::from(0xff_ffffu32) * denominator
    {
        return Err(Error::FloatRange(key.into()));
    }
    Ok(())
}

pub fn decimal_range(raw: &str, value: f32, key: &str) -> Result<(), Error> {
    if !value.is_finite() {
        return Err(Error::FloatRange(key.into()));
    }
    if value.abs() > f32::MIN_POSITIVE {
        return Ok(());
    }
    let unsigned = raw.trim_start_matches(['+', '-']);
    let (mantissa, power) = unsigned.split_once(['e', 'E']).unwrap_or((unsigned, "0"));
    let fraction = mantissa
        .split_once('.')
        .map_or(0, |(_, fraction)| fraction.len());
    let digits: Vec<_> = mantissa
        .bytes()
        .filter(|byte| byte.is_ascii_digit())
        .collect();
    let numerator = BigUint::parse_bytes(&digits, 10).ok_or(Error::Numeric)?;
    if numerator.is_zero() {
        return Ok(());
    }
    if value == 0. {
        return Err(Error::FloatRange(key.into()));
    }
    let power = exponent(power) - i64::try_from(fraction).map_err(|_| Error::Numeric)?;
    let (numerator, denominator) = if power < 0 {
        (
            numerator,
            BigUint::from(10u8).pow(u32::try_from(-power).map_err(|_| Error::Numeric)?),
        )
    } else {
        (
            numerator * BigUint::from(10u8).pow(u32::try_from(power).map_err(|_| Error::Numeric)?),
            BigUint::from(1u8),
        )
    };
    range(&numerator, &denominator, value, key)
}

pub fn hex(raw: &str, negative: bool, key: &str) -> Result<Option<f32>, Error> {
    let mut end = 0;
    let bytes = raw.as_bytes();
    let mut digits = Vec::new();
    let mut fraction = 0usize;
    let mut dot = false;
    while let Some(byte) = bytes.get(end) {
        if byte.is_ascii_hexdigit() {
            digits.push(*byte);
            if dot {
                fraction += 1;
            }
        } else if *byte == b'.' && !dot {
            dot = true;
        } else {
            break;
        }
        end += 1;
    }
    if digits.is_empty() {
        return Ok(None);
    }
    let power = if matches!(bytes.get(end), Some(b'p' | b'P')) {
        exponent(raw.get(end + 1..).ok_or(Error::Numeric)?)
    } else {
        0
    };
    let power = power - 4 * i64::try_from(fraction).map_err(|_| Error::Numeric)?;
    let mantissa = BigUint::parse_bytes(&digits, 16).ok_or(Error::Numeric)?;
    let sign = if negative { 0x8000_0000 } else { 0 };
    if mantissa.is_zero() {
        return Ok(Some(f32::from_bits(sign)));
    }
    let leading = i64::try_from(mantissa.bits()).map_err(|_| Error::Numeric)? - 1;
    let exponent = leading + power;
    if !(-150..=127).contains(&exponent) {
        return Err(Error::FloatRange(key.into()));
    }
    let shift = if exponent < -126 {
        -149 - power
    } else {
        leading - 23
    };
    let (mut significand, remainder, half) = if shift > 0 {
        let shift = usize::try_from(shift).map_err(|_| Error::Numeric)?;
        let remainder = &mantissa & ((BigUint::from(1u8) << shift) - BigUint::from(1u8));
        (
            &mantissa >> shift,
            remainder,
            BigUint::from(1u8) << (shift - 1),
        )
    } else {
        (
            &mantissa << usize::try_from(-shift).map_err(|_| Error::Numeric)?,
            BigUint::zero(),
            BigUint::zero(),
        )
    };
    if !remainder.is_zero()
        && (remainder > half
            || (remainder == half && (&significand & BigUint::from(1u8)) == BigUint::from(1u8)))
    {
        significand += BigUint::from(1u8);
    }
    let significand = significand.to_u32().ok_or(Error::Numeric)?;
    let bits = if exponent < -126 {
        significand
    } else {
        u32::try_from(exponent + 126).map_err(|_| Error::Numeric)? * 0x80_0000 + significand
    };
    let value = f32::from_bits(sign | bits);
    let (numerator, denominator) = if power < 0 {
        (
            mantissa,
            BigUint::from(1u8) << usize::try_from(-power).map_err(|_| Error::Numeric)?,
        )
    } else {
        (
            mantissa << usize::try_from(power).map_err(|_| Error::Numeric)?,
            BigUint::from(1u8),
        )
    };
    range(&numerator, &denominator, value, key)?;
    Ok(Some(value))
}
