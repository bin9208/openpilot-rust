use crate::Error;
use std::{collections::BTreeMap, fmt::Write};

fn token(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii()
                && !byte.is_ascii_control()
                && !b"()<>@,;:\\\"/[]?={} \t".contains(&byte)
        })
}

fn quoted(value: &str) -> Result<bool, Error> {
    if value.is_empty() {
        return Err(Error::Source("string index out of range".into()));
    }
    Ok(value.starts_with('"') && value.ends_with('"'))
}

fn unescape(value: &str) -> String {
    let mut output = String::new();
    let mut chars = value.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '\\' && chars.peek().is_some_and(char::is_ascii) {
            if let Some(next) = chars.next() {
                output.push(next);
            }
        } else {
            output.push(character);
        }
    }
    output
}

fn unquoted(value: &str) -> &str {
    value.get(1..value.len().saturating_sub(1)).unwrap_or("")
}

fn decode_extended(value: &str) -> Result<String, Error> {
    let mut fields = value.splitn(3, '\'');
    let encoding = fields.next().unwrap_or("");
    let _language = fields.next();
    let text = fields
        .next()
        .ok_or_else(|| Error::Source("not enough values to unpack (expected 3, got 2)".into()))?;
    if !text.contains('%') {
        return Ok(text.to_owned());
    }
    let encoding = if encoding.is_empty() {
        "utf-8"
    } else {
        encoding
    };
    let bytes = percent_encoding::percent_decode_str(text).collect::<Vec<_>>();
    match crate::request_text::decode(&bytes, encoding) {
        Ok(value) => Ok(value.into_owned()),
        Err(Error::UnknownCharset(_)) => {
            let decoder = encoding_rs::Encoding::for_label(encoding.as_bytes())
                .ok_or_else(|| Error::Source(format!("unknown encoding: {encoding}")))?;
            decoder
                .decode_without_bom_handling_and_without_replacement(&bytes)
                .map(std::borrow::Cow::into_owned)
                .ok_or_else(|| {
                    Error::Source(format!(
                        "'{encoding}' codec can't decode extended parameter"
                    ))
                })
        }
        Err(error) => Err(error),
    }
}

pub(super) fn name(header: Option<&[u8]>) -> Result<Option<String>, Error> {
    let Some(header) = header else {
        return Ok(None);
    };
    let header = String::from_utf8_lossy(header);
    let mut parts = header.split(';');
    if !token(parts.next().unwrap_or("")) {
        return Ok(None);
    }
    let mut parts = parts.peekable();
    let mut params = BTreeMap::new();
    while let Some(item) = parts.next() {
        if item.is_empty() {
            continue;
        }
        let Some((key, value)) = item.split_once('=') else {
            return Ok(None);
        };
        let key = key.to_lowercase().trim().to_owned();
        let value = value.trim_start();
        if params.contains_key(&key) {
            return Ok(None);
        }
        if !token(&key) {
            continue;
        }
        let tail = key
            .split_once('*')
            .map(|(_, tail)| tail.strip_suffix('*').unwrap_or(tail));
        let continuous = tail
            .is_some_and(|tail| !tail.is_empty() && tail.bytes().all(|byte| byte.is_ascii_digit()));
        let decoded = if continuous {
            if quoted(value)? {
                unescape(unquoted(value))
            } else if token(value) {
                value.to_owned()
            } else {
                continue;
            }
        } else if key.ends_with('*') {
            if !token(value) || value.matches('\'').count() != 2 {
                continue;
            }
            match decode_extended(value) {
                Ok(value) => value,
                Err(Error::Source(message)) if message.contains("codec can't decode") => continue,
                Err(error) => return Err(error),
            }
        } else if quoted(value)? {
            unescape(unquoted(value).trim_start_matches(['\\', '/']))
        } else if token(value) {
            value.to_owned()
        } else if let Some(next) = parts.peek() {
            let combined = format!("{value};{next}");
            if !quoted(&combined)? {
                return Ok(None);
            }
            parts.next();
            unescape(unquoted(&combined).trim_start_matches(['\\', '/']))
        } else {
            return Ok(None);
        };
        params.insert(key, decoded);
    }
    if let Some(value) = params.get("name*").or_else(|| params.get("name")) {
        return Ok(Some(value.clone()));
    }
    let mut assembled = String::new();
    for (index, (key, value)) in params
        .iter()
        .filter(|(key, _)| key.starts_with("name*"))
        .enumerate()
    {
        let tail = key.strip_prefix("name*").unwrap_or("");
        if tail.strip_suffix('*').unwrap_or(tail) != index.to_string() {
            break;
        }
        assembled.push_str(value);
    }
    if assembled.is_empty() {
        return Ok(None);
    }
    if assembled.contains('\'') {
        decode_extended(&assembled).map(Some)
    } else {
        Ok(Some(assembled))
    }
}

pub(super) fn bytes_repr(bytes: &[u8]) -> Result<String, Error> {
    let quote = if bytes.contains(&b'\'') && !bytes.contains(&b'"') {
        b'"'
    } else {
        b'\''
    };
    let mut output = String::from("b");
    output.push(char::from(quote));
    for &byte in bytes {
        match byte {
            b'\t' => output.push_str("\\t"),
            b'\r' => output.push_str("\\r"),
            b'\n' => output.push_str("\\n"),
            b'\\' => output.push_str("\\\\"),
            byte if byte == quote => {
                output.push('\\');
                output.push(char::from(byte));
            }
            32..=126 => output.push(char::from(byte)),
            _ => {
                write!(output, "\\x{byte:02x}").map_err(|error| Error::Source(error.to_string()))?
            }
        }
    }
    output.push(char::from(quote));
    Ok(output)
}
