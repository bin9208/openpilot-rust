//! UTF-7 replacement adapter adapted from charset-norm 3.5.1, codecs/utf.rs.
//! Upstream revision ea979ee8a7d907ba7ce17ef50755d8bfe7c46141; MIT license in
//! CHARSET-NORM-LICENSE. Code points retain Python surrogates until JSON/Params conversion.
use crate::Error;
fn is_base64(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'+' || byte == b'/'
}

fn from_base64(byte: u8) -> u32 {
    match byte {
        b'A'..=b'Z' => u32::from(byte - b'A'),
        b'a'..=b'z' => u32::from(byte - b'a') + 26,
        b'0'..=b'9' => u32::from(byte - b'0') + 52,
        b'+' => 62,
        _ => 63,
    }
}

pub fn decode_replace(data: &[u8]) -> Vec<u32> {
    let mut out = Vec::with_capacity(data.len());
    let mut position = 0usize;
    let mut in_shift = false;
    let mut bits = 0u32;
    let mut buffer = 0u64;
    let mut surrogate = 0u32;

    macro_rules! fail {
        () => {{
            out.push(0xfffd);
        }};
    }

    while position < data.len() {
        let byte = data[position];
        if in_shift {
            if is_base64(byte) {
                buffer = (buffer << 6) | u64::from(from_base64(byte));
                bits += 6;
                position += 1;
                if bits >= 16 {
                    let unit = u32::try_from((buffer >> (bits - 16)) & 0xFFFF).unwrap_or_default();
                    bits -= 16;
                    buffer &= (1u64 << bits) - 1;
                    if surrogate != 0 {
                        if (0xDC00..0xE000).contains(&unit) {
                            let value = 0x10000 + ((surrogate - 0xD800) << 10) + (unit - 0xDC00);
                            out.push(value);
                            surrogate = 0;
                            continue;
                        }
                        out.push(surrogate); // CPython retains unpaired UTF-7 surrogates.
                        surrogate = 0;
                    }
                    if (0xD800..0xDC00).contains(&unit) {
                        surrogate = unit;
                    } else {
                        out.push(unit);
                    }
                }
            } else {
                in_shift = false;
                if bits > 0 {
                    if bits >= 6 {
                        position += 1;
                        fail!(); // partial character in shift sequence
                        continue;
                    } else if buffer != 0 {
                        position += 1;
                        fail!(); // non-zero padding bits in shift sequence
                        continue;
                    }
                }
                if surrogate != 0 && byte <= 127 && byte != b'+' {
                    out.push(surrogate); // CPython retains unpaired UTF-7 surrogates.
                }
                surrogate = 0;
                if byte == b'-' {
                    position += 1;
                }
            }
        } else if byte == b'+' {
            position += 1;
            if position < data.len() && data[position] == b'-' {
                position += 1;
                out.push(u32::from(b'+'));
            } else if position < data.len() && !is_base64(data[position]) {
                position += 1;
                fail!(); // ill-formed sequence
            } else {
                in_shift = true;
                surrogate = 0;
                bits = 0;
                buffer = 0;
            }
        } else if byte <= 127 {
            position += 1;
            out.push(u32::from(byte));
        } else {
            position += 1;
            fail!(); // unexpected special character
        }
    }
    if in_shift && (surrogate != 0 || bits >= 6 || (bits > 0 && buffer != 0)) {
        // Unterminated shift sequence.
        fail!();
    }
    out
}

/// Escape non-scalar Python code points without changing JSON's error boundary.
pub fn json_text(points: &[u32]) -> Result<String, Error> {
    let mut text = String::new();
    let mut escaped = false;
    for &point in points {
        if let Some(character) = char::from_u32(point) {
            text.push(character);
            escaped = character == '\\' && !escaped;
        } else if (0xd800..=0xdfff).contains(&point) {
            if escaped {
                return Err(Error::Contract("invalid JSON escape before surrogate"));
            }
            text.push_str(&format!("\\u{point:04x}"));
            escaped = false;
        } else {
            return Err(Error::Contract("invalid decoded response code point"));
        }
    }
    Ok(text)
}
