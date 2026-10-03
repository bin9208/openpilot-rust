use super::Error;
use openpilot_params::Params;

pub fn read(params: &Params, key: &str) -> Result<f64, Error> {
    let Some(bytes) = params.get(key)? else {
        return Ok(0.);
    };
    if bytes.is_empty() {
        return Ok(0.);
    }
    let value = std::str::from_utf8(&bytes)?.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let mut end = 0;
    let bytes = value.as_bytes();
    if matches!(bytes.first(), Some(b'+' | b'-')) {
        end += 1;
    }
    let start = end;
    for name in ["infinity", "inf", "nan"] {
        if value[start..].to_ascii_lowercase().starts_with(name) {
            let sign = if bytes.first() == Some(&b'-') {
                0x8000_0000
            } else {
                0
            };
            let number = if name == "nan" {
                super::float_number::nan(&value[start + 3..], sign)
            } else {
                f32::from_bits(sign | 0x7f80_0000)
            };
            return Ok(f64::from(number));
        }
    }
    if value
        .get(start..start + 2)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("0x"))
    {
        if let Some(number) =
            super::float_number::hex(&value[start + 2..], bytes.first() == Some(&b'-'), key)?
        {
            return Ok(f64::from(number));
        }
    }
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    if bytes.get(end) == Some(&b'.') {
        end += 1;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
    }
    let mantissa_end = end;
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        end += 1;
        if matches!(bytes.get(end), Some(b'+' | b'-')) {
            end += 1;
        }
        let exponent_start = end;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
        if end == exponent_start {
            end = mantissa_end;
        }
    }
    let result = value[..end].parse::<f32>()?;
    super::float_number::decimal_range(&value[..end], result, key)?;
    Ok(f64::from(result))
}
