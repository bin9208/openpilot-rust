//! Python-compatible Unix filesystem strings in JSON, preserving surrogateescape bytes.
use crate::seal::Result;
use openpilot_log_format::{JsonValue, JsonView};
use std::{
    ffi::{OsStr, OsString},
    os::unix::ffi::{OsStrExt, OsStringExt},
};
/// Decode valid UTF-8 normally and map each undecodable byte to U+DC80..U+DCFF,
/// matching Python's Unix filesystem surrogateescape decoder.
pub fn codepoints(value: &OsStr) -> Vec<u32> {
    let mut points = Vec::new();
    let mut rest = value.as_bytes();
    while !rest.is_empty() {
        match std::str::from_utf8(rest) {
            Ok(text) => {
                points.extend(text.chars().map(u32::from));
                break;
            }
            Err(error) => {
                let valid = error.valid_up_to();
                points.extend(
                    std::str::from_utf8(&rest[..valid])
                        .expect("validated prefix")
                        .chars()
                        .map(u32::from),
                );
                rest = &rest[valid..];
                let count = error.error_len().unwrap_or(rest.len());
                points.extend(rest[..count].iter().map(|b| 0xdc00 + u32::from(*b)));
                rest = &rest[count..];
            }
        }
    }
    points
}
/// Preserve decoded nodes through writing; reparsing surrogate escape text can
/// combine adjacent surrogates and change Python filesystem string identity.
pub fn json_value(value: &OsStr) -> JsonValue {
    JsonValue::codepoints(codepoints(value))
        .expect("filesystem decoder emits valid Python codepoints")
}
pub fn json_string(value: &OsStr) -> Result<String> {
    Ok(json_value(value).to_json()?)
}
/// Decode a JSON filesystem string without replacing surrogateescape bytes.
/// Reject strings that cannot round-trip through Python's Unix filesystem codec.
pub fn from_json(value: &JsonValue) -> Result<OsString> {
    let JsonView::Text(points) = value.view() else {
        return Err("expected filesystem JSON string".into());
    };
    let mut bytes = Vec::new();
    for &point in points {
        if (0xdc80..=0xdcff).contains(&point) {
            bytes.push((point - 0xdc00) as u8);
        } else {
            let character = char::from_u32(point).ok_or("invalid filesystem codepoint")?;
            let mut utf8 = [0; 4];
            bytes.extend_from_slice(character.encode_utf8(&mut utf8).as_bytes());
        }
    }
    let result = OsString::from_vec(bytes);
    if codepoints(&result).as_slice() != points {
        return Err("filesystem JSON round trip failed".into());
    }
    Ok(result)
}
