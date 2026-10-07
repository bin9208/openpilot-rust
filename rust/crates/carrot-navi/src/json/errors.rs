use crate::Error;
use openpilot_logmessaged::JsonError;

fn context(text: &str, offset: usize) -> (Option<usize>, usize) {
    let mut quote = None;
    let mut escaped = false;
    let mut depth = 0_usize;
    for (index, character) in text.char_indices().take_while(|(index, _)| *index < offset) {
        if quote.is_some() {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                quote = None;
            }
        } else {
            match character {
                '"' => quote = Some(index),
                '[' | '{' => depth += 1,
                ']' | '}' => depth = depth.saturating_sub(1),
                _ => (),
            }
        }
    }
    (quote, depth)
}

fn number_start(text: &str, offset: usize) -> usize {
    text.as_bytes()
        .get(..offset)
        .unwrap_or_default()
        .iter()
        .rposition(|byte| !matches!(byte, b'0'..=b'9' | b'e' | b'E' | b'+' | b'-' | b'.'))
        .map_or(0, |position| position + 1)
}

fn location(text: &str, offset: usize, reason: &str) -> (usize, &'static str) {
    if text.starts_with('\u{feff}') {
        return (0, "Unexpected UTF-8 BOM (decode using utf-8-sig)");
    }
    let (quote, depth) = context(text, offset);
    match reason {
        "expected digit" => (number_start(text, offset), "Expecting value"),
        "expected fraction" | "expected exponent" => {
            let start = number_start(text, offset);
            let marker = if reason == "expected fraction" {
                b'.'
            } else {
                b'e'
            };
            let position = text
                .as_bytes()
                .get(start..offset)
                .unwrap_or_default()
                .iter()
                .position(|byte| byte.to_ascii_lowercase() == marker)
                .map_or(offset, |position| position + start);
            (
                position,
                if depth == 0 {
                    "Extra data"
                } else {
                    "Expecting ',' delimiter"
                },
            )
        }
        "unterminated string" | "unterminated escape" => {
            (quote.unwrap_or(offset), "Unterminated string starting at")
        }
        "invalid escape" => (offset.saturating_sub(2), "Invalid \\escape"),
        "invalid Unicode escape" => (
            text.get(..offset)
                .and_then(|text| text.rfind("\\u"))
                .map_or(offset, |position| position + 1),
            "Invalid \\uXXXX escape",
        ),
        "unescaped control character" => (offset, "Invalid control character at"),
        "expected colon" => (offset, "Expecting ':' delimiter"),
        "expected array delimiter" | "expected object delimiter" => {
            (offset, "Expecting ',' delimiter")
        }
        "expected string" => (offset, "Expecting property name enclosed in double quotes"),
        "trailing data" => (offset, "Extra data"),
        _ => (offset, "Expecting value"),
    }
}

pub(super) fn syntax(text: &str, error: JsonError) -> Error {
    match error {
        JsonError::Syntax { offset, reason } => {
            let (offset, reason) = location(text, offset, reason);
            let prefix = text.get(..offset).unwrap_or(text);
            let position = prefix.chars().count();
            let line = prefix
                .chars()
                .filter(|&character| character == '\n')
                .count()
                + 1;
            let column = prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1;
            Error::typed(
                "JSONDecodeError",
                format!("{reason}: line {line} column {column} (char {position})"),
            )
        }
        JsonError::IntegerLimit => {
            Error::value("Exceeds the limit (4300 digits) for integer string conversion")
        }
        JsonError::Message => Error::value("message must be a JSON object"),
    }
}
