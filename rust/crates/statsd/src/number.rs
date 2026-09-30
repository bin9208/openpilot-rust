use crate::Error;
pub(crate) fn repr(value: f64) -> Result<String, Error> {
    let mut output = String::new();
    openpilot_runtime_core::python_float::write_float(value, &mut output)?;
    Ok(output.replace("NaN", "nan").replace("Infinity", "inf"))
}
pub(crate) fn sum(values: &[f64]) -> f64 {
    let mut result = 0.0 + values[0];
    let mut correction = 0.0;
    for &value in &values[1..] {
        let next = result + value;
        correction += if result.abs() >= value.abs() {
            (result - next) + value
        } else {
            (value - next) + result
        };
        result = next;
    }
    if correction != 0.0 && correction.is_finite() {
        result += correction;
    }
    result
}
// Unicode 15.0 Nd zero codepoints, generated from the Python 3.12 source oracle.
const DECIMAL_ZERO: &[u32] = &[
    48, 1632, 1776, 1984, 2406, 2534, 2662, 2790, 2918, 3046, 3174, 3302, 3430, 3558, 3664, 3792,
    3872, 4160, 4240, 6112, 6160, 6470, 6608, 6784, 6800, 6992, 7088, 7232, 7248, 42528, 43216,
    43264, 43472, 43504, 43600, 44016, 65296, 66720, 68912, 69734, 69872, 69942, 70096, 70384,
    70736, 70864, 71248, 71360, 71472, 71904, 72016, 72784, 73040, 73120, 73552, 92768, 92864,
    93008, 120782, 120792, 120802, 120812, 120822, 123200, 123632, 124144, 125264, 130032,
];
pub(crate) fn parse(source: &str) -> Option<f64> {
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
