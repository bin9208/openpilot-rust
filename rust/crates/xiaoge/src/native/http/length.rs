use std::fmt::Write;

fn quoted(value: &str) -> Result<String, std::fmt::Error> {
    let quote = if value.contains('\'') && !value.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut output = String::new();
    output.push(quote);
    for character in value.chars() {
        match character {
            '\\' => output.push_str("\\\\"),
            '\t' => output.push_str("\\t"),
            '\r' => output.push_str("\\r"),
            '\n' => output.push_str("\\n"),
            value if value == quote => {
                output.push('\\');
                output.push(value);
            }
            value if value.is_control() || value.escape_debug().to_string().starts_with("\\u{") => {
                write!(output, "\\x{:02x}", u32::from(value))?;
            }
            value => output.push(value),
        }
    }
    output.push(quote);
    Ok(output.chars().take(200).collect())
}

pub fn parse(bytes: &[u8]) -> Result<usize, String> {
    let text: String = bytes.iter().map(|byte| char::from(*byte)).collect();
    let stripped = text.trim();
    let digits = stripped.strip_prefix(['+', '-']).unwrap_or(stripped);
    let mut previous = false;
    let mut count = 0usize;
    let mut number = Some(0usize);
    for byte in digits.bytes() {
        if byte == b'_' && previous {
            previous = false;
        } else if byte.is_ascii_digit() {
            previous = true;
            count += 1;
            number = number.and_then(|number| {
                number
                    .checked_mul(10)?
                    .checked_add(usize::from(byte - b'0'))
            });
        } else {
            previous = false;
            break;
        }
    }
    if !previous || count == 0 {
        return Err(format!(
            "invalid literal for int() with base 10: {}",
            quoted(&text).map_err(|error| error.to_string())?
        ));
    }
    if count > 4300 {
        return Err(format!("Exceeds the limit (4300 digits) for integer string conversion: value has {count} digits; use sys.set_int_max_str_digits() to increase the limit"));
    }
    number
        .filter(|number| !stripped.starts_with('-') && (1..=65536).contains(number))
        .ok_or_else(|| "request body must be 1 to 65536 bytes".to_owned())
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn header_retains_original_integer_syntax_and_error_kind() {
        assert_eq!(parse(b"+2").unwrap(), 2);
        assert_eq!(parse(b"2_0").unwrap(), 20);
        assert_eq!(
            parse(b"invalid").unwrap_err(),
            "invalid literal for int() with base 10: 'invalid'"
        );
        assert_eq!(
            parse(b"-2").unwrap_err(),
            "request body must be 1 to 65536 bytes"
        );
        assert!(parse("9".repeat(4301).as_bytes())
            .unwrap_err()
            .contains("value has 4301 digits"));
        assert_eq!(
            parse("x".repeat(205).as_bytes()).unwrap_err(),
            format!(
                "invalid literal for int() with base 10: '{}",
                "x".repeat(199)
            )
        );
    }
}
