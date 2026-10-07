use super::Error;

pub(super) fn integer_limit(text: &str) -> Result<(), Error> {
    let bytes = text.as_bytes();
    let mut index = 0;
    let mut quoted = false;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' if quoted => index += 2,
            b'"' => {
                quoted = !quoted;
                index += 1;
            }
            b'0'..=b'9' if !quoted => {
                let start = index;
                while bytes.get(index).is_some_and(|byte| byte.is_ascii_digit()) {
                    index += 1;
                }
                if bytes
                    .get(index)
                    .is_some_and(|byte| matches!(byte, b'.' | b'e' | b'E'))
                {
                    while bytes.get(index).is_some_and(|byte| {
                        byte.is_ascii_digit() || matches!(byte, b'.' | b'e' | b'E' | b'+' | b'-')
                    }) {
                        index += 1;
                    }
                } else if index - start > 4300 {
                    return Err(Error::IntegerDigits);
                }
            }
            _ => index += 1,
        }
    }
    Ok(())
}

pub(super) fn decode(mut bytes: &[u8]) -> Result<String, Error> {
    let (width, little) = if bytes.starts_with(&[255, 254, 0, 0]) {
        bytes = &bytes[4..];
        (4, true)
    } else if bytes.starts_with(&[0, 0, 254, 255]) {
        bytes = &bytes[4..];
        (4, false)
    } else if bytes.starts_with(&[255, 254]) {
        bytes = &bytes[2..];
        (2, true)
    } else if bytes.starts_with(&[254, 255]) {
        bytes = &bytes[2..];
        (2, false)
    } else if bytes.len() >= 4 && bytes[..3] == [0, 0, 0] {
        (4, false)
    } else if bytes.len() >= 4 && bytes[1..4] == [0, 0, 0] {
        (4, true)
    } else if bytes.len() >= 2 && bytes[0] == 0 {
        (2, false)
    } else if bytes.len() >= 2 && bytes[1] == 0 {
        (2, true)
    } else {
        (1, false)
    };
    if width == 1 {
        if bytes.starts_with(&[239, 187, 191]) {
            bytes = &bytes[3..];
        }
        let mut decoded = bytes.to_vec();
        for index in 0..decoded.len().saturating_sub(2) {
            if decoded[index] == 0xed
                && (0xa0..=0xbf).contains(&decoded[index + 1])
                && (0x80..=0xbf).contains(&decoded[index + 2])
            {
                decoded[index..index + 3].copy_from_slice(&[0xef, 0xbf, 0xbd]);
            }
        }
        return Ok(std::str::from_utf8(&decoded)?.to_owned());
    }
    let mut chunks = bytes.chunks_exact(width);
    let value = if width == 2 {
        let units: Vec<_> = chunks
            .by_ref()
            .map(|bytes| {
                if little {
                    u16::from_le_bytes([bytes[0], bytes[1]])
                } else {
                    u16::from_be_bytes([bytes[0], bytes[1]])
                }
            })
            .collect();
        char::decode_utf16(units)
            .map(|character| character.unwrap_or(char::REPLACEMENT_CHARACTER))
            .collect()
    } else {
        let mut result = String::new();
        for bytes in chunks.by_ref() {
            let bytes = [bytes[0], bytes[1], bytes[2], bytes[3]];
            let code = if little {
                u32::from_le_bytes(bytes)
            } else {
                u32::from_be_bytes(bytes)
            };
            let character = if (0xd800..=0xdfff).contains(&code) {
                char::REPLACEMENT_CHARACTER
            } else {
                char::from_u32(code).ok_or(Error::Encoding)?
            };
            result.push(character);
        }
        result
    };
    if !chunks.remainder().is_empty() {
        return Err(Error::Encoding);
    }
    Ok(value)
}

fn unicode(chars: &[char]) -> Option<u32> {
    if chars.len() < 6 || chars[..2] != ['\\', 'u'] {
        return None;
    }
    chars[2..6].iter().try_fold(0, |value, character| {
        character.to_digit(16).map(|digit| value * 16 + digit)
    })
}

pub(super) fn python_literals(text: &str) -> String {
    let chars: Vec<_> = text.chars().collect();
    let mut index = 0;
    let mut quoted = false;
    let mut output = String::new();
    while index < chars.len() {
        let remaining = &chars[index..];
        if quoted && chars[index] == '\\' {
            if let Some(code) = unicode(remaining) {
                let paired = (0xd800..=0xdbff).contains(&code)
                    && remaining
                        .get(6..)
                        .and_then(unicode)
                        .is_some_and(|code| (0xdc00..=0xdfff).contains(&code));
                if (0xd800..=0xdfff).contains(&code) && !paired {
                    output.push_str("\\ufffd");
                    index += 6;
                    continue;
                }
                let length = if paired { 12 } else { 6 };
                output.extend(remaining[..length].iter());
                index += length;
                continue;
            }
            output.push(chars[index]);
            index += 1;
            if let Some(&character) = chars.get(index) {
                output.push(character);
                index += 1;
            }
            continue;
        }
        if !quoted {
            let token = ["NaN", "Infinity", "-Infinity"].into_iter().find(|token| {
                remaining
                    .iter()
                    .copied()
                    .take(token.len())
                    .eq(token.chars())
            });
            if let Some(token) = token {
                output.push_str("\"__nonfinite__\"");
                index += token.len();
                continue;
            }
        }
        let character = chars[index];
        if character == '"' {
            quoted = !quoted;
        }
        output.push(character);
        index += 1;
    }
    output
}
