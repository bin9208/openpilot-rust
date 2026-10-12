use crate::{Error, Value};
use std::path::Path;

pub(super) fn trim(text: &str, limit: usize) -> String {
    let text = text.replace('\0', " ");
    let mut chars = text.chars();
    let mut clipped: String = chars.by_ref().take(limit).collect();
    if chars.next().is_some() {
        clipped.push_str("...");
    }
    clipped
}
pub(super) fn read(path: &Path, limit: usize) -> String {
    std::fs::read(path)
        .map(|bytes| {
            String::from_utf8_lossy(&bytes)
                .chars()
                .take(limit)
                .collect()
        })
        .unwrap_or_default()
}
pub(super) fn lines(text: &str) -> Vec<String> {
    let mut rows = Vec::new();
    let mut row = String::new();
    let mut after_cr = false;
    for character in text.chars() {
        if after_cr && character == '\n' {
            after_cr = false;
            continue;
        }
        after_cr = character == '\r';
        if matches!(
            character,
            '\n' | '\r' | '\u{b}' | '\u{c}' | '\u{1c}'
                ..='\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}'
        ) {
            rows.push(std::mem::take(&mut row));
        } else {
            row.push(character);
        }
    }
    if !row.is_empty() {
        rows.push(row);
    }
    rows
}
pub(super) fn tail(path: &Path, limit: usize) -> Value {
    let mut rows = lines(&read(path, 256_000));
    rows.drain(..rows.len().saturating_sub(limit));
    Value::Array(
        rows.iter()
            .map(|line| Value::text(&trim(line, 800)))
            .collect(),
    )
}
pub(super) fn input(value: &Value) -> Result<String, Error> {
    if !value.truth() {
        return Ok(String::new());
    }
    match value {
        Value::Text(points) => Ok(points
            .iter()
            .map(|point| char::from_u32(*point).unwrap_or('?'))
            .collect()),
        value => value.py_string()?.string().map_err(Error::from),
    }
}
