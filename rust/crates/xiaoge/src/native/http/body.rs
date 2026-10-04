use openpilot_logmessaged::{JsonError, JsonValue};
use std::fmt::Write;

fn codepoint(code: u32, output: &mut String) -> Result<(), String> {
    if (0xd800..=0xdfff).contains(&code) {
        write!(output, "\\u{code:04x}").map_err(|error| error.to_string())?;
    } else {
        output.push(
            char::from_u32(code).ok_or_else(|| "invalid JSON character encoding".to_owned())?,
        );
    }
    Ok(())
}

fn decode(mut bytes: &[u8]) -> Result<String, String> {
    let (width, little) = if let Some(rest) = bytes.strip_prefix(b"\xff\xfe\0\0") {
        bytes = rest;
        (4, true)
    } else if let Some(rest) = bytes.strip_prefix(b"\0\0\xfe\xff") {
        bytes = rest;
        (4, false)
    } else if let Some(rest) = bytes.strip_prefix(b"\xff\xfe") {
        bytes = rest;
        (2, true)
    } else if let Some(rest) = bytes.strip_prefix(b"\xfe\xff") {
        bytes = rest;
        (2, false)
    } else if bytes.len() >= 4 && bytes[..3] == [0; 3] {
        (4, false)
    } else if bytes.len() >= 4 && bytes[1..4] == [0; 3] {
        (4, true)
    } else if bytes.len() >= 2 && bytes[0] == 0 {
        (2, false)
    } else if bytes.len() >= 2 && bytes[1] == 0 {
        (2, true)
    } else {
        (1, false)
    };
    if width == 1 {
        bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
        let mut decoded = Vec::with_capacity(bytes.len());
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] == 0xed
                && bytes
                    .get(index + 1)
                    .is_some_and(|byte| (0xa0..=0xbf).contains(byte))
                && bytes
                    .get(index + 2)
                    .is_some_and(|byte| (0x80..=0xbf).contains(byte))
            {
                let code = ((u32::from(bytes[index] & 15)) << 12)
                    | (u32::from(bytes[index + 1] & 63) << 6)
                    | u32::from(bytes[index + 2] & 63);
                decoded.extend_from_slice(format!("\\u{code:04x}").as_bytes());
                index += 3;
            } else {
                decoded.push(bytes[index]);
                index += 1;
            }
        }
        return String::from_utf8(decoded).map_err(|error| error.to_string());
    }
    if !bytes.len().is_multiple_of(width) {
        return Err("truncated JSON character encoding".to_owned());
    }
    let mut output = String::new();
    if width == 4 {
        for bytes in bytes.chunks_exact(4) {
            let bytes: [u8; 4] = bytes
                .try_into()
                .map_err(|_| "invalid UTF-32 word".to_owned())?;
            codepoint(
                if little {
                    u32::from_le_bytes(bytes)
                } else {
                    u32::from_be_bytes(bytes)
                },
                &mut output,
            )?;
        }
    } else {
        let mut words = bytes
            .chunks_exact(2)
            .map(|bytes| {
                if little {
                    u16::from_le_bytes([bytes[0], bytes[1]])
                } else {
                    u16::from_be_bytes([bytes[0], bytes[1]])
                }
            })
            .peekable();
        while let Some(high) = words.next() {
            if (0xd800..=0xdbff).contains(&high)
                && words
                    .peek()
                    .is_some_and(|low| (0xdc00..=0xdfff).contains(low))
            {
                let low = words
                    .next()
                    .ok_or_else(|| "truncated UTF-16 pair".to_owned())?;
                codepoint(
                    0x10000 + ((u32::from(high) - 0xd800) << 10) + u32::from(low) - 0xdc00,
                    &mut output,
                )?;
            } else {
                codepoint(u32::from(high), &mut output)?;
            }
        }
    }
    Ok(output)
}

fn unfinished_quote(source: &str) -> Option<usize> {
    let mut start = None;
    let mut escaped = false;
    for (index, byte) in source.bytes().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        if byte == b'\\' && start.is_some() {
            escaped = true;
        }
        if byte == b'"' {
            start = if start.is_some() { None } else { Some(index) };
        }
    }
    start
}

fn oversized_integer(source: &str) -> Option<usize> {
    let mut quoted = false;
    let mut escaped = false;
    let mut digits = 0usize;
    for byte in source.bytes().chain(std::iter::once(b' ')) {
        if escaped {
            escaped = false;
            continue;
        }
        if byte == b'\\' && quoted {
            escaped = true;
            continue;
        }
        if byte == b'"' {
            quoted = !quoted;
        }
        if !quoted && byte.is_ascii_digit() {
            digits += 1;
        } else {
            if digits > 4300 && !matches!(byte, b'.' | b'e' | b'E') {
                return Some(digits);
            }
            digits = 0;
        }
    }
    None
}

pub fn json(bytes: &[u8]) -> Result<JsonValue, String> {
    let source = decode(bytes)?;
    JsonValue::parse(&source).map_err(|error| match error {
        JsonError::Syntax { mut offset, reason } => {
            if matches!(reason, "unterminated string" | "unterminated escape") {
                offset = unfinished_quote(&source).unwrap_or(offset);
            }
            let message = match reason {
                "expected value" | "expected digit" => "Expecting value",
                "expected string" => "Expecting property name enclosed in double quotes",
                "expected colon" => "Expecting ':' delimiter",
                "expected array delimiter" | "expected object delimiter" => {
                    "Expecting ',' delimiter"
                }
                "trailing data" => "Extra data",
                "unterminated string" | "unterminated escape" => "Unterminated string starting at",
                "invalid escape" => "Invalid \\escape",
                "invalid Unicode escape" => "Invalid \\uXXXX escape",
                "unescaped control character" => "Invalid control character at",
                _ => "Expecting value",
            };
            let prefix = &source[..offset];
            let character = prefix.chars().count();
            let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
            let column = prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1;
            format!("{message}: line {line} column {column} (char {character})")
        }
        JsonError::IntegerLimit => oversized_integer(&source).map_or_else(|| JsonError::IntegerLimit.to_string(), |digits| {
            format!("Exceeds the limit (4300 digits) for integer string conversion: value has {digits} digits; use sys.set_int_max_str_digits() to increase the limit")
        }),
        JsonError::Message => JsonError::Message.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::json;

    #[test]
    fn unterminated_string_reports_its_opening_quote() {
        // Given a JSON string whose end is missing.
        let body = b"[\"unfinished";
        // When parsing the actual request body.
        let error = json(body).err().unwrap();
        // Then the original decoder identifies the opening quote, not end-of-input.
        assert_eq!(
            error,
            "Unterminated string starting at: line 1 column 2 (char 1)"
        );
    }

    #[test]
    fn oversized_integer_reports_original_digit_count() {
        // Given a JSON integer above the source's default digit limit.
        let body = format!("{{\"threshold\": -{}}}", "9".repeat(4302));
        // When parsing the actual request body.
        let error = json(body.as_bytes()).err().unwrap();
        // Then the response retains the source's exact limit and observed count.
        assert_eq!(error, "Exceeds the limit (4300 digits) for integer string conversion: value has 4302 digits; use sys.set_int_max_str_digits() to increase the limit");
    }
}
