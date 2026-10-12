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

fn unicode_error(codec: &str, start: usize, length: usize, bytes: &[u8], reason: &str) -> Error {
    let position = if length == 1 {
        format!("byte 0x{:02x} in position {start}", bytes[start])
    } else {
        format!("bytes in position {start}-{}", start + length - 1)
    };
    Error::Source(format!("'{codec}' codec can't decode {position}: {reason}"))
}

fn unicode_words(bytes: &[u8], encoding: &str, width: usize) -> Result<String, Error> {
    let normalized = encoding.replace('_', "");
    let mut little = normalized.ends_with("le");
    let mut offset = 0;
    let bom_mode = normalized == "utf16" || normalized == "utf32";
    if bom_mode {
        if bytes.starts_with(if width == 2 {
            &[0xff, 0xfe][..]
        } else {
            &[0xff, 0xfe, 0, 0][..]
        }) {
            little = true;
            offset = width;
        } else if bytes.starts_with(if width == 2 {
            &[0xfe, 0xff][..]
        } else {
            &[0, 0, 0xfe, 0xff][..]
        }) {
            little = false;
            offset = width;
        } else {
            little = cfg!(target_endian = "little");
        }
    }
    let codec = format!("utf-{}-{}", width * 8, if little { "le" } else { "be" });
    let mut output = String::new();
    while offset < bytes.len() {
        if bytes.len() - offset < width {
            return Err(unicode_error(
                &codec,
                offset,
                bytes.len() - offset,
                bytes,
                "truncated data",
            ));
        }
        let mut word = 0u32;
        for index in 0..width {
            let shift = if little { index } else { width - 1 - index } * 8;
            word |= u32::from(bytes[offset + index]) << shift;
        }
        let mut consumed = width;
        if width == 2 && (0xd800..=0xdbff).contains(&word) {
            if bytes.len() - offset < 4 {
                return Err(unicode_error(
                    &codec,
                    offset,
                    bytes.len() - offset,
                    bytes,
                    "unexpected end of data",
                ));
            }
            let low = if little {
                u16::from_le_bytes([bytes[offset + 2], bytes[offset + 3]])
            } else {
                u16::from_be_bytes([bytes[offset + 2], bytes[offset + 3]])
            };
            if !(0xdc00..=0xdfff).contains(&low) {
                return Err(unicode_error(
                    &codec,
                    offset,
                    2,
                    bytes,
                    "illegal UTF-16 surrogate",
                ));
            }
            word = 0x10000 + ((word - 0xd800) << 10) + (u32::from(low) - 0xdc00);
            consumed = 4;
        } else if (0xd800..=0xdfff).contains(&word) {
            return Err(unicode_error(
                &codec,
                offset,
                width,
                bytes,
                if width == 2 {
                    "illegal encoding"
                } else {
                    "code point in surrogate code point range(0xd800, 0xe000)"
                },
            ));
        }
        let character = char::from_u32(word).ok_or_else(|| {
            unicode_error(
                &codec,
                offset,
                width,
                bytes,
                "code point not in range(0x110000)",
            )
        })?;
        output.push(character);
        offset += consumed;
    }
    Ok(output)
}

pub(crate) fn decode<'a>(bytes: &'a [u8], encoding: &str) -> Result<Cow<'a, str>, Error> {
    let normalized = encoding.to_ascii_lowercase().replace(['-', ' '], "_");
    match normalized.as_str() {
        "utf_8" | "utf8" | "u8" => std::str::from_utf8(bytes)
            .map(Cow::Borrowed)
            .map_err(|error| utf8_error(bytes, error)),
        "utf_8_sig" | "utf8_sig" => {
            let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
            std::str::from_utf8(bytes)
                .map(Cow::Borrowed)
                .map_err(|error| utf8_error(bytes, error))
        }
        "utf_16" | "utf16" | "utf_16le" | "utf_16_le" | "utf16le" | "utf_16be" | "utf_16_be"
        | "utf16be" => unicode_words(bytes, &normalized, 2).map(Cow::Owned),
        "utf_32" | "utf32" | "utf_32le" | "utf_32_le" | "utf32le" | "utf_32be" | "utf_32_be"
        | "utf32be" => unicode_words(bytes, &normalized, 4).map(Cow::Owned),
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
        _ => {
            let label = encoding.replace('_', "-");
            let decoder = encoding_rs::Encoding::for_label(label.as_bytes())
                .ok_or_else(|| Error::UnknownCharset(encoding.to_owned()))?;
            decoder
                .decode_without_bom_handling_and_without_replacement(bytes)
                .ok_or_else(|| {
                    Error::Source(format!(
                        "'{}' codec can't decode request content",
                        decoder.name()
                    ))
                })
        }
    }
}
