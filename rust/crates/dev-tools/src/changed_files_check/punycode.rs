//! CPython3.12.15 encodings/punycode.py decode algorithm, Python/PSF license.
//! Rust scalar output is the strict-UTF8 boundary: Python surrogate results fail.
use super::{Diagnostic, Phase};
fn invalid() -> Diagnostic {
    Diagnostic::new(Phase::Decode, "invalid strict Punycode payload")
}
fn adapt(mut delta: u128, first: bool, count: u128) -> u128 {
    delta /= if first { 700 } else { 2 };
    delta += delta / count;
    let mut divisions = 0;
    while delta > 455 {
        delta /= 35;
        divisions += 36;
    }
    divisions + 36 * delta / (delta + 38)
}
pub(super) fn decode(raw: &[u8]) -> Result<String, Diagnostic> {
    if !raw.is_ascii() {
        return Err(invalid());
    }
    let (basic, extended) = raw
        .iter()
        .rposition(|&v| v == b'-')
        .map_or((&[][..], raw), |at| (&raw[..at], &raw[at + 1..]));
    let mut output: Vec<char> = basic.iter().map(|&v| char::from(v)).collect();
    let mut code = 128u128;
    let mut position = 0u128;
    let mut bias = 72u128;
    let mut cursor = 0;
    let mut first = true;
    while cursor < extended.len() {
        let mut delta = 0u128;
        let mut weight = 1u128;
        let mut k = 36u128;
        loop {
            let digit = match extended
                .get(cursor)
                .copied()
                .map(|v| v.to_ascii_uppercase())
            {
                Some(v @ b'A'..=b'Z') => u128::from(v - b'A'),
                Some(v @ b'0'..=b'9') => u128::from(v - b'0') + 26,
                _ => return Err(invalid()),
            };
            cursor += 1;
            delta = delta
                .checked_add(digit.checked_mul(weight).ok_or_else(invalid)?)
                .ok_or_else(invalid)?;
            let threshold = k.saturating_sub(bias).clamp(1, 26);
            if digit < threshold {
                break;
            }
            weight = weight.checked_mul(36 - threshold).ok_or_else(invalid)?;
            k = k.checked_add(36).ok_or_else(invalid)?;
        }
        let count = output.len() as u128 + 1;
        position = position.checked_add(delta).ok_or_else(invalid)?;
        code = code.checked_add(position / count).ok_or_else(invalid)?;
        let value = u32::try_from(code)
            .ok()
            .and_then(char::from_u32)
            .ok_or_else(invalid)?;
        let at = usize::try_from(position % count).map_err(|_| invalid())?;
        output.insert(at, value);
        position = at as u128 + 1;
        bias = adapt(delta, first, count);
        first = false;
    }
    Ok(output.into_iter().collect())
}
