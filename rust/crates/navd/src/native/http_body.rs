use crate::route::RequestError;
use std::io::{self, Read};

pub(super) fn decompress(mut bytes: Vec<u8>, encoding: &str) -> io::Result<Vec<u8>> {
    if matches!(encoding, "gzip" | "br") {
        return Ok(bytes);
    }
    let lower = encoding.to_lowercase();
    let modes: Vec<_> = lower.split(',').map(str::trim).collect();
    if !modes
        .iter()
        .any(|mode| matches!(*mode, "gzip" | "x-gzip" | "br" | "deflate"))
    {
        return Ok(bytes);
    }
    if modes.len() > 5 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "too many content encodings",
        ));
    }
    for mode in modes.into_iter().rev() {
        let mut decoded = Vec::new();
        match mode {
            "gzip" | "x-gzip" => {
                flate2::read::MultiGzDecoder::new(bytes.as_slice()).read_to_end(&mut decoded)?;
            }
            "br" => {
                brotli_decompressor::Decompressor::new(bytes.as_slice(), 4096)
                    .read_to_end(&mut decoded)?;
            }
            _ => {
                if flate2::read::ZlibDecoder::new(bytes.as_slice())
                    .read_to_end(&mut decoded)
                    .is_err()
                {
                    decoded.clear();
                    flate2::read::DeflateDecoder::new(bytes.as_slice())
                        .read_to_end(&mut decoded)?;
                }
            }
        }
        bytes = decoded;
    }
    Ok(bytes)
}

pub(super) fn body_json(
    bytes: &[u8],
    content_type: &str,
    fallback: &str,
) -> Result<serde_json::Value, RequestError> {
    let specified = content_type.contains("text")
        || content_type.contains("application/json")
        || content_type.split(';').skip(1).any(|item| {
            item.trim()
                .split_once('=')
                .is_some_and(|(name, _)| name.eq_ignore_ascii_case("charset"))
        });
    if !specified && bytes.len() > 3 {
        if let Some(encoding) = json_encoding(bytes) {
            match charset_norm::codecs::decode(
                bytes,
                encoding,
                charset_norm::codecs::Errors::Strict,
            ) {
                Ok(text) => return serde_json::from_str(&text).map_err(RequestError::from),
                Err(charset_norm::codecs::DecodeError::Invalid) => {}
                Err(charset_norm::codecs::DecodeError::Unknown) => {
                    return Err(RequestError::Transport("JSON encoding unavailable".into()))
                }
            }
        }
    }
    serde_json::from_str(fallback).map_err(RequestError::from)
}

fn json_encoding(bytes: &[u8]) -> Option<&'static str> {
    let first = bytes.get(..4)?;
    if first.starts_with(b"\x00\x00\xfe\xff") || first.starts_with(b"\xff\xfe\x00\x00") {
        return Some("utf-32");
    }
    if first.starts_with(b"\xef\xbb\xbf") {
        return Some("utf-8-sig");
    }
    if first.starts_with(b"\xff\xfe") || first.starts_with(b"\xfe\xff") {
        return Some("utf-16");
    }
    match first.iter().filter(|&&byte| byte == 0).count() {
        0 => Some("utf-8"),
        2 if first[0] == 0 && first[2] == 0 => Some("utf-16-be"),
        2 if first[1] == 0 && first[3] == 0 => Some("utf-16-le"),
        3 if first[..3] == [0, 0, 0] => Some("utf-32-be"),
        3 if first[1..] == [0, 0, 0] => Some("utf-32-le"),
        _ => None,
    }
}
