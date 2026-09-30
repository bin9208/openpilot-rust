use crate::value::Value;

pub fn whitespace(value: char) -> bool {
    value.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&value)
}

pub fn convert(value: &Value) -> Option<i128> {
    match value {
        Value::Integer(value) => value.parse().ok(),
        Value::Bool(value) => Some(if *value { 1 } else { 0 }),
        Value::Float(value) if value.is_finite() => format!("{:.0}", value.trunc()).parse().ok(),
        Value::Text(bytes) => {
            let text = std::str::from_utf8(bytes).ok()?.trim();
            let mut normalized = String::new();
            let mut previous_digit = false;
            for (index, character) in text.chars().enumerate() {
                match character {
                    '+' | '-' if index == 0 => normalized.push(character),
                    '_' if previous_digit => previous_digit = false,
                    _ => {
                        let code = u32::from(character);
                        let start = DECIMAL_STARTS
                            .iter()
                            .find(|&&start| (start..start + 10).contains(&code))?;
                        normalized.push(char::from_digit(code - start, 10)?);
                        previous_digit = true;
                    }
                }
            }
            if previous_digit {
                normalized.parse().ok()
            } else {
                None
            }
        }
        Value::Null | Value::Float(_) | Value::Array(_) | Value::Object(_) => None,
    }
}

// Unicode 15.0 decimal-zero codepoints, matching the source CPython 3.12 tables.
const DECIMAL_STARTS: &[u32] = &[
    0x30, 0x660, 0x6f0, 0x7c0, 0x966, 0x9e6, 0xa66, 0xae6, 0xb66, 0xbe6, 0xc66, 0xce6, 0xd66,
    0xde6, 0xe50, 0xed0, 0xf20, 0x1040, 0x1090, 0x17e0, 0x1810, 0x1946, 0x19d0, 0x1a80, 0x1a90,
    0x1b50, 0x1bb0, 0x1c40, 0x1c50, 0xa620, 0xa8d0, 0xa900, 0xa9d0, 0xa9f0, 0xaa50, 0xabf0, 0xff10,
    0x104a0, 0x10d30, 0x11066, 0x110f0, 0x11136, 0x111d0, 0x112f0, 0x11450, 0x114d0, 0x11650,
    0x116c0, 0x11730, 0x118e0, 0x11950, 0x11c50, 0x11d50, 0x11da0, 0x11f50, 0x16a60, 0x16ac0,
    0x16b50, 0x1d7ce, 0x1d7d8, 0x1d7e2, 0x1d7ec, 0x1d7f6, 0x1e140, 0x1e2f0, 0x1e4f0, 0x1e950,
    0x1fbf0,
];
