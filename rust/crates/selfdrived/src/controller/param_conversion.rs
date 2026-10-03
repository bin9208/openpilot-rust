/// Python's decimal `int(bytes)` result, retained without a native integer range limit.
pub fn decimal(bytes: &[u8]) -> Option<String> {
    let whitespace = |byte: &u8| matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c);
    let begin = bytes.iter().position(|byte| !whitespace(byte))?;
    let end = bytes.iter().rposition(|byte| !whitespace(byte))? + 1;
    let bytes = &bytes[begin..end];
    let (negative, digits) = match bytes.first()? {
        b'-' => (true, &bytes[1..]),
        b'+' => (false, &bytes[1..]),
        _ => (false, bytes),
    };
    let mut output = String::new();
    for (index, byte) in digits.iter().enumerate() {
        match byte {
            b'0'..=b'9' => output.push(char::from(*byte)),
            b'_' if index > 0
                && digits[index - 1].is_ascii_digit()
                && digits.get(index + 1).is_some_and(u8::is_ascii_digit) => {}
            _ => return None,
        }
    }
    if output.is_empty() || output.len() > 4300 {
        return None;
    }
    let normalized = output.trim_start_matches('0');
    Some(if normalized.is_empty() {
        "0".into()
    } else if negative {
        format!("-{normalized}")
    } else {
        normalized.into()
    })
}

pub fn bytes_repr(bytes: &[u8]) -> String {
    let quote = if bytes.contains(&b'\'') && !bytes.contains(&b'"') {
        b'"'
    } else {
        b'\''
    };
    let mut output = String::from("b");
    output.push(char::from(quote));
    for byte in bytes {
        match byte {
            b'\t' => output.push_str("\\t"),
            b'\n' => output.push_str("\\n"),
            b'\r' => output.push_str("\\r"),
            b'\\' => output.push_str("\\\\"),
            value if *value == quote => {
                output.push('\\');
                output.push(char::from(*value));
            }
            32..=126 => output.push(char::from(*byte)),
            value => output.push_str(&format!("\\x{value:02x}")),
        }
    }
    output.push(char::from(quote));
    output
}

/// Encoding detection used by `json.loads(bytes)`, including surrogatepass code units.
pub fn json_text(bytes: &[u8]) -> Option<String> {
    let (width, little, skip) = if bytes.starts_with(&[0, 0, 0xfe, 0xff]) {
        (4, false, 4)
    } else if bytes.starts_with(&[0xff, 0xfe, 0, 0]) {
        (4, true, 4)
    } else if bytes.starts_with(&[0xfe, 0xff]) {
        (2, false, 2)
    } else if bytes.starts_with(&[0xff, 0xfe]) {
        (2, true, 2)
    } else if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        (1, false, 3)
    } else if bytes.len() >= 4 {
        if bytes[0] == 0 {
            if bytes[1] == 0 {
                (4, false, 0)
            } else {
                (2, false, 0)
            }
        } else if bytes[1] == 0 {
            if bytes[2] == 0 && bytes[3] == 0 {
                (4, true, 0)
            } else {
                (2, true, 0)
            }
        } else {
            (1, false, 0)
        }
    } else if bytes.len() == 2 && bytes[0] == 0 {
        (2, false, 0)
    } else if bytes.len() == 2 && bytes[1] == 0 {
        (2, true, 0)
    } else {
        (1, false, 0)
    };
    let bytes = &bytes[skip..];
    if width == 1 {
        let mut remaining = bytes;
        let mut output = String::new();
        while !remaining.is_empty() {
            match std::str::from_utf8(remaining) {
                Ok(text) => {
                    output.push_str(text);
                    return Some(output);
                }
                Err(error) => {
                    let valid = error.valid_up_to();
                    output.push_str(std::str::from_utf8(&remaining[..valid]).ok()?);
                    remaining = &remaining[valid..];
                    if remaining.len() < 3
                        || remaining[0] != 0xed
                        || !(0xa0..=0xbf).contains(&remaining[1])
                        || !(0x80..=0xbf).contains(&remaining[2])
                    {
                        return None;
                    }
                    let point = (u32::from(remaining[0] & 15) << 12)
                        | (u32::from(remaining[1] & 63) << 6)
                        | u32::from(remaining[2] & 63);
                    output.push_str(&format!("\\u{point:04x}"));
                    remaining = &remaining[3..];
                }
            }
        }
        return Some(output);
    }
    if !bytes.len().is_multiple_of(width) {
        return None;
    }
    let mut points = Vec::new();
    for chunk in bytes.chunks_exact(width) {
        let point = if little {
            chunk
                .iter()
                .rev()
                .fold(0u32, |value, byte| (value << 8) | u32::from(*byte))
        } else {
            chunk
                .iter()
                .fold(0u32, |value, byte| (value << 8) | u32::from(*byte))
        };
        points.push(point);
    }
    let mut output = String::new();
    let mut index = 0;
    while index < points.len() {
        let mut point = points[index];
        index += 1;
        if width == 2
            && (0xd800..=0xdbff).contains(&point)
            && points
                .get(index)
                .is_some_and(|next| (0xdc00..=0xdfff).contains(next))
        {
            point = 0x10000 + ((point - 0xd800) << 10) + (points[index] - 0xdc00);
            index += 1;
        }
        if (0xd800..=0xdfff).contains(&point) {
            output.push_str(&format!("\\u{point:04x}"));
        } else {
            output.push(char::from_u32(point)?);
        }
    }
    Some(output)
}
