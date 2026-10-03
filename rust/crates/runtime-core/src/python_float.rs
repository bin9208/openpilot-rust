use std::fmt::Write;
pub fn write_float(value: f64, output: &mut String) -> std::fmt::Result {
    if value.is_nan() {
        output.push_str("NaN");
        return Ok(());
    }
    if value.is_infinite() {
        output.push_str(if value.is_sign_positive() {
            "Infinity"
        } else {
            "-Infinity"
        });
        return Ok(());
    }
    // Schubfach's shortest even-tie digits match Python dtoa; Rust Debug
    // formatting chooses the other decimal on some exact halfway values.
    let mut buffer = zmij::Buffer::new();
    let text = buffer.format_finite(value);
    let unsigned = if let Some(text) = text.strip_prefix('-') {
        output.push('-');
        text
    } else {
        text
    };
    if value == 0. {
        output.push_str("0.0");
        return Ok(());
    }
    let (mantissa, exponent) = match unsigned.split_once('e') {
        Some((mantissa, exponent)) => (
            mantissa,
            exponent.parse::<i32>().map_err(|_| std::fmt::Error)?,
        ),
        None => (unsigned, 0),
    };
    let mut position = i32::try_from(mantissa.find('.').unwrap_or(mantissa.len()))
        .map_err(|_| std::fmt::Error)?
        + exponent;
    let digits: String = mantissa
        .chars()
        .filter(|&character| character != '.')
        .collect();
    let significant = digits.trim_start_matches('0');
    position -= i32::try_from(digits.len() - significant.len()).map_err(|_| std::fmt::Error)?;
    let significant = significant.trim_end_matches('0');
    let scientific = position - 1;
    if !(-4..16).contains(&scientific) {
        let (first, rest) = significant.split_at(1);
        output.push_str(first);
        if !rest.is_empty() {
            output.push('.');
            output.push_str(rest);
        }
        write!(output, "e{scientific:+03}")?;
    } else if position <= 0 {
        output.push_str("0.");
        output.extend(std::iter::repeat_n(
            '0',
            usize::try_from(-position).map_err(|_| std::fmt::Error)?,
        ));
        output.push_str(significant);
    } else {
        let position = usize::try_from(position).map_err(|_| std::fmt::Error)?;
        if position >= significant.len() {
            output.push_str(significant);
            output.extend(std::iter::repeat_n('0', position - significant.len()));
            output.push_str(".0");
        } else {
            let (first, rest) = significant.split_at(position);
            output.push_str(first);
            output.push('.');
            output.push_str(rest);
        }
    }
    Ok(())
}

// Unicode 15.0 Nd zero codepoints, generated from the Python 3.12 source oracle.
const DECIMAL_ZERO: &[u32] = &[
    48, 1632, 1776, 1984, 2406, 2534, 2662, 2790, 2918, 3046, 3174, 3302, 3430, 3558, 3664, 3792,
    3872, 4160, 4240, 6112, 6160, 6470, 6608, 6784, 6800, 6992, 7088, 7232, 7248, 42528, 43216,
    43264, 43472, 43504, 43600, 44016, 65296, 66720, 68912, 69734, 69872, 69942, 70096, 70384,
    70736, 70864, 71248, 71360, 71472, 71904, 72016, 72784, 73040, 73120, 73552, 92768, 92864,
    93008, 120782, 120792, 120802, 120812, 120822, 123200, 123632, 124144, 125264, 130032,
];
pub fn parse(source: &str) -> Option<f64> {
    let mut text = String::new();
    for character in source.trim_matches(char::is_whitespace).chars() {
        let point = u32::from(character);
        if let Some(zero) = DECIMAL_ZERO
            .iter()
            .find(|&&zero| point >= zero && point < zero + 10)
        {
            text.push(char::from_digit(point - zero, 10)?);
        } else {
            text.push(character);
        }
    }
    let bytes = text.as_bytes();
    for (index, &byte) in bytes.iter().enumerate() {
        if byte == b'_'
            && (index == 0
                || index + 1 == bytes.len()
                || !bytes[index - 1].is_ascii_digit()
                || !bytes[index + 1].is_ascii_digit())
        {
            return None;
        }
    }
    text.retain(|character| character != '_');
    let unsigned = text.trim_start_matches(['+', '-']);
    if unsigned.eq_ignore_ascii_case("nan")
        || unsigned.eq_ignore_ascii_case("inf")
        || unsigned.eq_ignore_ascii_case("infinity")
    {
        return text.parse().ok();
    }
    if text
        .bytes()
        .any(|byte| !matches!(byte, b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-'))
    {
        return None;
    }
    text.parse().ok()
}
