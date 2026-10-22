//! CPython3.12.15 Objects/unicodeobject.c UTF7 state transitions, PSF license.
//! Final output is strict UTF8, so unpaired Python surrogate values are rejected.
use super::{Diagnostic, Phase};
fn invalid() -> Diagnostic {
    Diagnostic::new(Phase::Decode, "invalid strict UTF7 payload")
}
fn base64(b: u8) -> Option<u32> {
    match b {
        b'A'..=b'Z' => Some(u32::from(b - b'A')),
        b'a'..=b'z' => Some(u32::from(b - b'a') + 26),
        b'0'..=b'9' => Some(u32::from(b - b'0') + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}
pub(super) fn decode(raw: &[u8]) -> Result<String, Diagnostic> {
    let mut output = String::new();
    let mut i = 0;
    while let Some(&b) = raw.get(i) {
        if b != b'+' {
            if !b.is_ascii() {
                return Err(invalid());
            }
            output.push(char::from(b));
            i += 1;
            continue;
        }
        i += 1;
        if raw.get(i) == Some(&b'-') {
            output.push('+');
            i += 1;
            continue;
        }
        if raw.get(i).is_some_and(|&b| base64(b).is_none()) {
            return Err(invalid());
        }
        let mut bits = 0u32;
        let mut buffer = 0u32;
        let mut high = None;
        while let Some(value) = raw.get(i).copied().and_then(base64) {
            buffer = (buffer << 6) | value;
            bits += 6;
            i += 1;
            if bits >= 16 {
                bits -= 16;
                let unit = buffer >> bits;
                buffer &= (1 << bits) - 1;
                if let Some(first) = high.take() {
                    if !(0xdc00..=0xdfff).contains(&unit) {
                        return Err(invalid());
                    }
                    let cp = 0x10000 + ((first - 0xd800) << 10) + (unit - 0xdc00);
                    output.push(char::from_u32(cp).ok_or_else(invalid)?);
                } else if (0xd800..=0xdbff).contains(&unit) {
                    high = Some(unit);
                } else {
                    output.push(char::from_u32(unit).ok_or_else(invalid)?);
                }
            }
        }
        if bits >= 6 || buffer != 0 || high.is_some() {
            return Err(invalid());
        }
        if raw.get(i) == Some(&b'-') {
            i += 1;
        }
    }
    Ok(output)
}
