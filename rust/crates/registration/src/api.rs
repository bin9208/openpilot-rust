use crate::Error;
use openpilot_http_transport::socket_timeout_agent;
use std::{io::Read, path::Path, time::Duration};

pub struct Response {
    pub status: u16,
    pub text: String,
}

/// The registration subset of common/api.py: POST query, no request body, new session per call.
pub fn api_get(
    host: &str,
    source_root: &Path,
    endpoint: &str,
    params: &[(&str, Option<&str>)],
) -> Result<Response, Error> {
    let version = openpilot_runtime_version::get_version(source_root)?;
    let mut url = url::Url::parse(&format!("{host}/{endpoint}"))?;
    for (name, value) in params {
        if let Some(value) = value {
            url.query_pairs_mut().append_pair(name, value);
        }
    }
    let timeout = Duration::from_secs(15);
    let config = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .max_redirects(0)
        .max_redirects_will_error(false)
        .timeout_connect(Some(timeout))
        .timeout_global(None)
        .max_idle_connections(0)
        .user_agent(format!("openpilot-{version}"))
        .build();
    let agent = socket_timeout_agent(config, timeout);
    let mut method = ureq::http::Method::POST;
    let mut cookies = cookie_store::CookieStore::default();
    for redirects in 0..=30 {
        let mut request = ureq::http::Request::builder()
            .method(method.clone())
            .uri(url.as_str())
            .header("accept", "*/*")
            .header("accept-encoding", "gzip, deflate, br")
            .header("connection", "keep-alive");
        if method == ureq::http::Method::POST {
            request = request.header("content-length", "0");
        }
        let mut matched = cookies.matches(&url);
        matched.retain(|cookie| cookie.secure() != Some(true) || url.scheme() == "https");
        matched.sort_by_key(|cookie| std::cmp::Reverse(cookie.path.as_ref().len()));
        let cookie_header = matched
            .into_iter()
            .map(|cookie| format!("{}={}", cookie.name(), cookie.value()))
            .collect::<Vec<_>>()
            .join("; ");
        if !cookie_header.is_empty() {
            request = request.header("cookie", cookie_header);
        }
        // ureq 3.4.2 formats whole Set-Cookie values as request cookies, including attributes.
        // Keep its jar empty and serialize only matched name/value pairs here.
        agent.cookie_jar_lock().clear();
        let mut response = agent.run(request.body(())?)?;
        for value in response.headers().get_all("set-cookie") {
            if let Ok(value) = value.to_str() {
                let _ = cookies.parse(value, &url);
            }
        }
        let status = response.status().as_u16();
        let location = response
            .headers()
            .get("location")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .to_owned();
        let deflate = response
            .headers()
            .get("content-encoding")
            .is_some_and(|value| value.as_bytes().eq_ignore_ascii_case(b"deflate"));
        let mut bytes = Vec::new();
        response.body_mut().as_reader().read_to_end(&mut bytes)?;
        if deflate {
            let mut decoded = Vec::new();
            if flate2::read::ZlibDecoder::new(bytes.as_slice())
                .read_to_end(&mut decoded)
                .is_err()
            {
                decoded.clear();
                flate2::read::DeflateDecoder::new(bytes.as_slice()).read_to_end(&mut decoded)?;
            }
            bytes = decoded;
        }
        if let Some(location) = location.filter(|_| matches!(status, 301 | 302 | 303 | 307 | 308)) {
            if redirects == 30 {
                return Err(Error::Contract("exceeded 30 redirects"));
            }
            url = url.join(&location)?;
            if matches!(status, 301..=303) {
                method = ureq::http::Method::GET;
            }
        } else {
            return Ok(Response {
                status,
                text: response_text(&bytes, &content_type)?,
            });
        }
    }
    Err(Error::Contract("redirect limit"))
}

pub fn response_text(bytes: &[u8], content_type: &str) -> Result<String, Error> {
    let charset = content_type.split(';').skip(1).find_map(|item| {
        let (name, value) = item.trim().split_once('=')?;
        name.eq_ignore_ascii_case("charset")
            .then(|| value.trim_matches(['\'', '"', ' ']))
    });
    let charset = charset
        .or_else(|| content_type.contains("text").then_some("ISO-8859-1"))
        .or_else(|| content_type.contains("application/json").then_some("UTF-8"));
    if charset.is_none() {
        let candidates = charset_norm::from_bytes(bytes);
        return match candidates.best() {
            Some(best) => Ok(best.decoded()?.to_owned()),
            None => Ok(String::from_utf8_lossy(bytes).into_owned()),
        };
    }
    decode_replace(bytes, charset.unwrap_or("UTF-8"))
}

fn decode_replace(bytes: &[u8], charset: &str) -> Result<String, Error> {
    use charset_norm::codecs::{self, DecodeError, Errors};
    match codecs::decode(bytes, charset, Errors::Strict) {
        Ok(text) => return Ok(text),
        Err(DecodeError::Unknown) => return Ok(String::from_utf8_lossy(bytes).into_owned()),
        Err(DecodeError::Invalid) => {}
    }
    if let Some(decode) = codecs::single_byte_decoder(charset) {
        return Ok(bytes
            .iter()
            .map(|&byte| decode(byte).unwrap_or(char::REPLACEMENT_CHARACTER))
            .collect());
    }
    let canonical = charset_norm::encoding::iana_name(charset, false)
        .map_err(|_| Error::Contract("response encoding name is invalid"))?;
    match canonical.as_str() {
        "utf_7" => crate::utf7::json_text(&crate::utf7::decode_replace(bytes)),
        "utf_8" => Ok(String::from_utf8_lossy(bytes).into_owned()),
        "utf_8_sig" => Ok(String::from_utf8_lossy(
            bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes),
        )
        .into_owned()),
        "utf_16" | "utf_16_le" | "utf_16_be" => Ok(unicode_replace(bytes, &canonical, 2)),
        "utf_32" | "utf_32_le" | "utf_32_be" => Ok(unicode_replace(bytes, &canonical, 4)),
        _ => {
            let encoding = encoding_rs::Encoding::for_label(canonical.as_bytes())
                .or_else(|| {
                    charset_norm::encoding::aliases(&canonical)
                        .into_iter()
                        .find_map(|name| encoding_rs::Encoding::for_label(name.as_bytes()))
                })
                .ok_or(Error::Contract(
                    "native replacement decoder unavailable for declared response encoding",
                ))?;
            Ok(encoding.decode_without_bom_handling(bytes).0.into_owned())
        }
    }
}

fn unicode_replace(mut bytes: &[u8], encoding: &str, width: usize) -> String {
    let mut little = !encoding.ends_with("_be");
    if !encoding.ends_with("_le") && !encoding.ends_with("_be") {
        let (little_bom, big_bom): (&[u8], &[u8]) = if width == 2 {
            (b"\xff\xfe", b"\xfe\xff")
        } else {
            (b"\xff\xfe\x00\x00", b"\x00\x00\xfe\xff")
        };
        if let Some(rest) = bytes.strip_prefix(little_bom) {
            bytes = rest;
            little = true;
        } else if let Some(rest) = bytes.strip_prefix(big_bom) {
            bytes = rest;
            little = false;
        } else {
            little = cfg!(target_endian = "little");
        }
    }
    if width == 2 {
        let mut chunks = bytes.chunks_exact(2);
        let units: Vec<_> = chunks
            .by_ref()
            .map(|pair| {
                if little {
                    u16::from_le_bytes([pair[0], pair[1]])
                } else {
                    u16::from_be_bytes([pair[0], pair[1]])
                }
            })
            .collect();
        let mut text = String::from_utf16_lossy(&units);
        // A high surrogate followed by a partial unit is one truncated sequence in CPython.
        if !chunks.remainder().is_empty()
            && !units
                .last()
                .is_some_and(|unit| (0xd800..0xdc00).contains(unit))
        {
            text.push(char::REPLACEMENT_CHARACTER);
        }
        text
    } else {
        let mut chunks = bytes.chunks_exact(4);
        let mut text: String = chunks
            .by_ref()
            .map(|quad| {
                let value = if little {
                    u32::from_le_bytes([quad[0], quad[1], quad[2], quad[3]])
                } else {
                    u32::from_be_bytes([quad[0], quad[1], quad[2], quad[3]])
                };
                char::from_u32(value).unwrap_or(char::REPLACEMENT_CHARACTER)
            })
            .collect();
        if !chunks.remainder().is_empty() {
            text.push(char::REPLACEMENT_CHARACTER);
        }
        text
    }
}
