use crate::Error;
use hyper::{header, HeaderMap};
use std::borrow::Cow;

pub(crate) fn encoding(headers: &HeaderMap) -> String {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            value.split(';').skip(1).find_map(|part| {
                let (name, value) = part.split_once('=').unwrap_or((part, ""));
                name.trim()
                    .eq_ignore_ascii_case("charset")
                    .then(|| value.trim_matches([' ', '"']))
            })
        })
        .filter(|value| !value.is_empty())
        .unwrap_or("utf-8")
        .to_owned()
}

fn utf8_error(bytes: &[u8], error: std::str::Utf8Error) -> Error {
    let start = error.valid_up_to();
    let length = error.error_len().unwrap_or(bytes.len() - start);
    let reason = if error.error_len().is_none() {
        "unexpected end of data"
    } else if matches!(bytes[start], 0xc2..=0xf4) {
        "invalid continuation byte"
    } else {
        "invalid start byte"
    };
    let position = if length == 1 {
        format!("byte 0x{:02x} in position {start}", bytes[start])
    } else {
        format!("bytes in position {start}-{}", start + length - 1)
    };
    Error::Source(format!("'utf-8' codec can't decode {position}: {reason}"))
}

pub(crate) fn decode<'a>(bytes: &'a [u8], encoding: &str) -> Result<Cow<'a, str>, Error> {
    let normalized = encoding.to_ascii_lowercase().replace(['-', ' '], "_");
    match normalized.as_str() {
        "utf_8" | "utf8" | "u8" => std::str::from_utf8(bytes)
            .map(Cow::Borrowed)
            .map_err(|error| utf8_error(bytes, error)),
        "latin_1" | "latin1" | "latin" | "iso_8859_1" | "iso8859_1" | "cp819" | "l1"
        | "iso_ir_100" | "csisolatin1" => Ok(Cow::Owned(
            bytes.iter().map(|byte| char::from(*byte)).collect(),
        )),
        "ascii" | "us_ascii" | "us" | "646" => {
            if let Some((position, byte)) =
                bytes.iter().enumerate().find(|(_, byte)| !byte.is_ascii())
            {
                return Err(Error::Source(format!("'ascii' codec can't decode byte 0x{byte:02x} in position {position}: ordinal not in range(128)")));
            }
            std::str::from_utf8(bytes)
                .map(Cow::Borrowed)
                .map_err(|error| utf8_error(bytes, error))
        }
        _ => Err(Error::Source(format!(
            "unsupported request charset: {encoding}"
        ))),
    }
}
