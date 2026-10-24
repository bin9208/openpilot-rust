//! Independent Requests-facing HTTP/query/text policy for the private QA server.
//! Only third-party codecs and compression primitives are shared with runtime.
use super::values as v;
use crate::seal::Result;
use openpilot_log_format::{JsonValue, JsonView};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::TcpStream,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use url::Url;

pub(super) fn encode(text: &str, encoding: &str) -> Result<Vec<u8>> {
    super::encoding::encode(text, encoding)
}
fn utf7(bytes: &[u8]) -> Vec<u32> {
    // CPython UTF-7's replacement consumer, retaining lone UTF-16 surrogates
    // rather than passing them through a Rust String. Shifted valid scalars are
    // combined once; JsonValue::parse_codepoints retains raw surrogate identity.
    let base = |b: u8| -> Option<u32> {
        match b {
            b'A'..=b'Z' => Some(u32::from(b - b'A')),
            b'a'..=b'z' => Some(u32::from(b - b'a' + 26)),
            b'0'..=b'9' => Some(u32::from(b - b'0' + 52)),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    };
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'+' {
            out.push(if bytes[i] < 128 {
                u32::from(bytes[i])
            } else {
                0xfffd
            });
            i += 1;
            continue;
        }
        i += 1;
        if bytes.get(i) == Some(&b'-') {
            out.push(u32::from(b'+'));
            i += 1;
            continue;
        }
        if i == bytes.len() {
            break;
        }
        if base(bytes[i]).is_none() {
            out.push(0xfffd);
            i += 1;
            continue;
        }
        let mut buffer = 0u64;
        let mut bits = 0;
        let mut high: Option<u32> = None;
        while i < bytes.len() {
            let Some(value) = base(bytes[i]) else { break };
            buffer = (buffer << 6) | u64::from(value);
            bits += 6;
            i += 1;
            if bits >= 16 {
                let unit = ((buffer >> (bits - 16)) & 0xffff) as u32;
                bits -= 16;
                buffer &= (1u64 << bits) - 1;
                if let Some(previous) = high.take() {
                    if (0xdc00..=0xdfff).contains(&unit) {
                        out.push(0x10000u32 + ((previous - 0xd800) << 10) + (unit - 0xdc00));
                        continue;
                    }
                    out.push(previous)
                }
                if (0xd800..=0xdbff).contains(&unit) {
                    high = Some(unit)
                } else {
                    out.push(unit)
                }
            }
        }
        let invalid = bits >= 6 || (bits > 0 && buffer != 0);
        if invalid {
            out.push(0xfffd);
            if i < bytes.len() {
                i += 1;
            }
        } else {
            if let Some(high) = high {
                out.push(high)
            }
            if bytes.get(i) == Some(&b'-') {
                i += 1;
            }
        }
    }
    out
}
fn decode(bytes: &[u8], encoding: &str) -> Result<Vec<u32>> {
    use charset_norm::codecs::{decode, Errors};
    let canonical = charset_norm::encoding::iana_name(encoding, false)?;
    if canonical == "utf_7" {
        return Ok(utf7(bytes));
    }
    if canonical.starts_with("utf_32") {
        let (little, skip) = if canonical.ends_with("_be") {
            (false, 0)
        } else if canonical.ends_with("_le") {
            (true, 0)
        } else if bytes.starts_with(&[0, 0, 0xfe, 0xff]) {
            (false, 4)
        } else {
            (
                true,
                usize::from(bytes.starts_with(&[0xff, 0xfe, 0, 0])) * 4,
            )
        };
        return Ok(bytes[skip..]
            .chunks(4)
            .map(|group| {
                if let Ok(raw) = <[u8; 4]>::try_from(group) {
                    let point = if little {
                        u32::from_le_bytes(raw)
                    } else {
                        u32::from_be_bytes(raw)
                    };
                    if point <= 0x10ffff && !(0xd800..=0xdfff).contains(&point) {
                        point
                    } else {
                        0xfffd
                    }
                } else {
                    0xfffd
                }
            })
            .collect());
    }
    if canonical == "utf_8" || canonical == "utf_8_sig" {
        let bytes = if canonical == "utf_8_sig" {
            bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes)
        } else {
            bytes
        };
        return Ok(String::from_utf8_lossy(bytes)
            .chars()
            .map(u32::from)
            .collect());
    }
    if canonical.starts_with("utf_16") {
        let (little, skip) = if canonical.ends_with("_be") {
            (false, 0)
        } else if canonical.ends_with("_le") {
            (true, 0)
        } else if bytes.starts_with(b"\xfe\xff") {
            (false, 2)
        } else {
            (true, usize::from(bytes.starts_with(b"\xff\xfe")) * 2)
        };
        let mut out = char::decode_utf16(bytes[skip..].chunks_exact(2).map(|raw| {
            if little {
                u16::from_le_bytes([raw[0], raw[1]])
            } else {
                u16::from_be_bytes([raw[0], raw[1]])
            }
        }))
        .map(|c| u32::from(c.unwrap_or('\u{fffd}')))
        .collect::<Vec<_>>();
        if !(bytes.len() - skip).is_multiple_of(2) {
            out.push(0xfffd)
        }
        return Ok(out);
    }
    if let Ok(text) = decode(bytes, &canonical, Errors::Strict) {
        return Ok(text.chars().map(u32::from).collect());
    }
    // Use the vendored CPython tables for valid individual multibyte characters;
    // invalid lead/trail bytes are consumed once and replaced, never ignored.
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let mut valid = None;
        for length in 1..=4.min(bytes.len() - i) {
            if let Ok(text) = decode(&bytes[i..i + length], &canonical, Errors::Strict) {
                if !text.is_empty() {
                    valid = Some((length, text));
                    break;
                }
            }
        }
        if let Some((length, text)) = valid {
            out.extend(text.chars().map(u32::from));
            i += length
        } else {
            out.push(0xfffd);
            i += 1
        }
    }
    Ok(out)
}
fn response_text(bytes: &[u8], headers: &[(String, String)]) -> Result<Vec<u32>> {
    let content = headers
        .iter()
        .rev()
        .find(|(key, _)| key.eq_ignore_ascii_case("content-type"))
        .map(|(_, v)| v.as_str());
    // Match Requests.utils._parse_content_type_header: MIME value remains
    // case-sensitive, parameter names are unquoted/lowercased, last key wins.
    let parsed = content.filter(|v| !v.is_empty()).map(|value| {
        let mut parts = value.split(';');
        let mime = parts.next().unwrap_or("").trim();
        let mut parameters = BTreeMap::new();
        for part in parts {
            if let Some((key, value)) = part.trim().split_once('=') {
                parameters.insert(
                    key.trim_matches(['\"', '\'', ' ']).to_lowercase(),
                    value.trim_matches(['\"', '\'', ' ']).to_owned(),
                );
            }
        }
        (mime, parameters)
    });
    let encoding = parsed
        .and_then(|(mime, parameters)| {
            parameters.get("charset").cloned().or_else(|| {
                if mime.contains("text") {
                    Some("ISO-8859-1".into())
                } else if mime.contains("application/json") {
                    Some("utf-8".into())
                } else {
                    None
                }
            })
        })
        .unwrap_or_else(|| {
            charset_norm::from_bytes(bytes).best().map_or_else(
                || "utf-8".into(),
                |v| {
                    // Requests calls charset_normalizer.legacy.detect, which
                    // adds _sig to UTF-8 when the detector found its BOM.
                    // The detector's encoding() alone deliberately omits it.
                    if v.encoding() == "utf_8" && v.has_sig_or_bom() {
                        "utf_8_sig".into()
                    } else {
                        v.encoding().to_owned()
                    }
                },
            )
        });
    if charset_norm::codecs::decode(&[], &encoding, charset_norm::codecs::Errors::Strict).is_err()
        && charset_norm::encoding::iana_name(&encoding, false)? != "utf_7"
    {
        return decode(bytes, "utf-8");
    }
    decode(bytes, &encoding)
}
fn sequence(value: &JsonValue) -> Option<Vec<JsonValue>> {
    match value.view() {
        JsonView::Array(values) => Some(values),
        JsonView::Object(values) => Some(
            values
                .iter()
                .map(|(key, _)| JsonValue::codepoints(key.to_vec()).expect("parsed Unicode key"))
                .collect(),
        ),
        _ => None,
    }
}
fn query(params: &[(String, JsonValue)]) -> Result<String> {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in params {
        for value in sequence(value).unwrap_or_else(|| vec![value.clone()]) {
            if v::is_null(&value) {
                continue;
            }
            for value in sequence(&value).unwrap_or_else(|| vec![value]) {
                serializer.append_pair(key, &v::scalar_utf8(&v::python_str(&value)?)?);
            }
        }
    }
    Ok(serializer.finish())
}
struct Response {
    status: i64,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}
fn exchange(url: &Url, method: &str, user_agent: &str, cookie: Option<&str>) -> Result<Response> {
    if url.scheme() != "http" || url.host_str() != Some("127.0.0.1") {
        return Err("registration source permits only its private loopback HTTP server".into());
    }
    let port = url.port_or_known_default().ok_or("HTTP port missing")?;
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    stream.set_read_timeout(Some(Duration::from_secs(15)))?;
    stream.set_write_timeout(Some(Duration::from_secs(15)))?;
    let mut target = url.path().to_owned();
    if let Some(query) = url.query() {
        target.push('?');
        target.push_str(query)
    }
    // Requests/urllib3 in the declared Python 3.12 CI dependency context has
    // gzip/deflate/brotli. An undeclared backports.zstd package adds a fourth
    // encoding; its distinct original capture remains provenance, not this
    // client's capability. Never advertise a decoder this client lacks.
    let mut request=format!("{method} {target} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nUser-Agent: {user_agent}\r\nAccept-Encoding: gzip, deflate, br\r\nAccept: */*\r\nConnection: keep-alive\r\n");
    if method == "POST" {
        request.push_str("Content-Length: 0\r\n")
    }
    if let Some(cookie) = cookie {
        request.push_str(&format!("Cookie: {cookie}\r\n"))
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes())?;
    let mut header = Vec::new();
    loop {
        let mut byte = [0];
        if stream.read(&mut byte)? == 0 {
            return Err("HTTP peer disconnected before response".into());
        }
        header.push(byte[0]);
        if header.ends_with(b"\r\n\r\n") {
            break;
        }
        if header.len() > 1024 * 1024 {
            return Err("HTTP response headers exceed fixture bound".into());
        }
    }
    let header = std::str::from_utf8(&header)?;
    let mut lines = header.split("\r\n");
    let status = lines
        .next()
        .ok_or("HTTP status absent")?
        .split_whitespace()
        .nth(1)
        .ok_or("malformed HTTP status")?
        .parse()?;
    let headers = lines
        .filter(|v| !v.is_empty())
        .map(|line| {
            let (key, value) = line.split_once(':').ok_or("malformed response header")?;
            Ok((key.to_owned(), value.trim_start().to_owned()))
        })
        .collect::<Result<Vec<_>>>()?;
    let length = headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("content-length"))
        .map(|(_, n)| n.parse::<usize>())
        .transpose()?;
    let mut body = Vec::new();
    match length {
        Some(length) => {
            if length > 32 * 1024 * 1024 {
                return Err("HTTP fixture body exceeds bound".into());
            }
            body.resize(length, 0);
            stream.read_exact(&mut body)?
        }
        None => {
            stream.read_to_end(&mut body)?;
        }
    }
    // Read timeout belongs to each blocking receive, so a 16s progressing body is
    // admitted while a 16s completely idle header/body raises and is retried.
    if let Some((_, encoding)) = headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("content-encoding"))
    {
        let mut output = Vec::new();
        match encoding.to_ascii_lowercase().as_str() {
            "gzip" => {
                flate2::read::GzDecoder::new(&body[..]).read_to_end(&mut output)?;
            }
            "deflate" => {
                if flate2::read::ZlibDecoder::new(&body[..])
                    .read_to_end(&mut output)
                    .is_err()
                {
                    output.clear();
                    flate2::read::DeflateDecoder::new(&body[..]).read_to_end(&mut output)?;
                }
            }
            "br" => {
                brotli_decompressor::Decompressor::new(&body[..], 4096).read_to_end(&mut output)?;
            }
            _ => {
                output = body;
            }
        }
        body = output;
    }
    Ok(Response {
        status,
        headers,
        body,
    })
}
#[derive(Clone)]
struct Cookie {
    name: String,
    value: String,
    path: String,
    domain: String,
    secure: bool,
    order: usize,
}
fn cookies(headers: &[(String, String)], url: &Url, jar: &mut Vec<Cookie>) {
    for (_, raw) in headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("set-cookie"))
    {
        let mut fields = raw.split(';');
        let Some((name, value)) = fields.next().and_then(|s| s.split_once('=')) else {
            continue;
        };
        let name = name.trim().to_owned();
        if name.is_empty() {
            continue;
        }
        let host = url.host_str().unwrap_or("");
        let mut domain = host.to_owned();
        let mut path = url
            .path()
            .rsplit_once('/')
            .map_or(
                "/",
                |(parent, _)| if parent.is_empty() { "/" } else { parent },
            )
            .to_owned();
        let mut secure = false;
        let mut expired = false;
        let mut rejected = false;
        for field in fields {
            let (key, value) = field.trim().split_once('=').unwrap_or((field.trim(), ""));
            match key.to_ascii_lowercase().as_str() {
                "path" if value.starts_with('/') => path = value.into(),
                "domain" => {
                    domain = value.trim_start_matches('.').to_ascii_lowercase();
                    if host != domain && !host.ends_with(&format!(".{domain}")) {
                        rejected = true
                    }
                }
                "secure" => secure = true,
                "max-age" => {
                    if let Ok(seconds) = value.parse::<i64>() {
                        expired = seconds <= 0
                    }
                }
                "expires" => {
                    if let Some(time) = http_date(value) {
                        expired = time
                            <= SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs() as i64;
                    }
                }
                _ => {}
            }
        }
        if rejected {
            continue;
        }
        let old = jar
            .iter()
            .position(|c| c.name == name && c.domain == domain && c.path == path);
        let order = old.map_or(jar.len(), |at| jar[at].order);
        if let Some(at) = old {
            jar.remove(at);
        }
        if !expired {
            jar.push(Cookie {
                name,
                value: value.trim().into(),
                domain,
                path,
                secure,
                order,
            });
        }
    }
}
fn http_date(value: &str) -> Option<i64> {
    let fields = value
        .split(|c: char| c.is_ascii_whitespace() || c == ',' || c == '-' || c == ':')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    let at = fields.iter().position(|v| {
        [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ]
        .contains(v)
    })?;
    if at == 0 || fields.len() < at + 5 {
        return None;
    }
    let day = fields[at - 1].parse::<i64>().ok()?;
    let month = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ]
    .iter()
    .position(|m| m == &fields[at])? as i64
        + 1;
    let mut year = fields[at + 1].parse::<i64>().ok()?;
    if year < 100 {
        year += if year >= 70 { 1900 } else { 2000 }
    }
    let hour = fields[at + 2].parse::<i64>().ok()?;
    let minute = fields[at + 3].parse::<i64>().ok()?;
    let second = fields[at + 4].parse::<i64>().ok()?;
    let y = year - i64::from(month <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * m + 2) / 5 + day - 1;
    let days = era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + day_of_year - 719468;
    Some(days * 86400 + hour * 3600 + minute * 60 + second)
}
fn cookie_header(url: &Url, jar: &[Cookie]) -> Option<String> {
    let mut valid = jar
        .iter()
        .filter(|c| {
            (!c.secure || url.scheme() == "https")
                && (url.host_str() == Some(c.domain.as_str())
                    || url
                        .host_str()
                        .is_some_and(|h| h.ends_with(&format!(".{}", c.domain))))
                && (url.path() == c.path
                    || url
                        .path()
                        .starts_with(&format!("{}/", c.path.trim_end_matches('/'))))
        })
        .collect::<Vec<_>>();
    valid.sort_by_key(|c| (std::cmp::Reverse(c.path.len()), c.order));
    (!valid.is_empty()).then(|| {
        valid
            .iter()
            .map(|c| format!("{}={}", c.name, c.value))
            .collect::<Vec<_>>()
            .join("; ")
    })
}
pub(super) fn authenticate(
    host: &str,
    params: &[(String, JsonValue)],
    user_agent: &str,
) -> Result<(i64, JsonValue)> {
    let mut url = Url::parse(&format!("{host}/v2/pilotauth/"))?;
    let query = query(params)?;
    if !query.is_empty() {
        url.set_query(Some(&query))
    }
    let mut method = "POST";
    let mut jar = Vec::new();
    let mut redirects = 0;
    loop {
        let response = exchange(
            &url,
            method,
            user_agent,
            cookie_header(&url, &jar).as_deref(),
        )?;
        cookies(&response.headers, &url, &mut jar);
        if matches!(response.status, 301..=303 | 307 | 308) {
            if let Some((_, location)) = response
                .headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("location"))
            {
                redirects += 1;
                if redirects > 30 {
                    return Err("too many redirects".into());
                }
                url = url.join(location)?;
                if matches!(response.status, 301..=303) {
                    method = "GET"
                }
                continue;
            }
        }
        if matches!(response.status, 402 | 403) {
            return Ok((response.status, v::null()));
        }
        let points = response_text(&response.body, &response.headers)?;
        if points.first() == Some(&0xfeff) || points.contains(&0) {
            return Err("JSON response has unexpected BOM or raw NUL".into());
        }
        return Ok((response.status, JsonValue::parse_codepoints(&points)?));
    }
}
pub(super) fn normalized(
    raw: &JsonValue,
    public_keys: &BTreeMap<String, String>,
    expiration: i64,
) -> Result<JsonValue> {
    let mut result = Vec::new();
    for request in v::items(raw)? {
        let line = v::required_text(&request, "line")?;
        let mut words = line.split_whitespace();
        let method = words.next().ok_or("request method absent")?;
        let target = words.next().ok_or("request target absent")?;
        if words.next().is_none() || words.next().is_some() {
            return Err("malformed HTTP request line".into());
        }
        let url = Url::parse(&format!("http://127.0.0.1{target}"))?;
        let mut query: BTreeMap<String, Vec<JsonValue>> = BTreeMap::new();
        for (key, value) in url.query_pairs() {
            query
                .entry(key.into_owned())
                .or_default()
                .push(v::text(&value));
        }
        let verified_algorithm = query.get("register_token").is_some_and(|v| !v.is_empty());
        if let Some(token) = query.get("register_token").and_then(|v| v.first()) {
            let (algorithm, claims, public) =
                super::keys::verify(&v::string(token)?, public_keys, expiration)?;
            let actual = query
                .get("public_key")
                .ok_or("signed request has no public key")?;
            if actual.len() != 1 || !actual[0].text_eq(&public) {
                return Err("registration public key differs from verified signing key".into());
            }
            query.insert("register_token".into(), vec![claims]);
            query.insert("algorithm".into(), vec![v::text(&algorithm)]);
            query.insert(
                "public_key".into(),
                vec![v::text("<verified-generated-public-key>")],
            );
        }
        let mut headers = BTreeMap::new();
        for pair in v::items(&v::get(&request, "headers")?)? {
            let pair = v::items(&pair)?;
            headers.insert(v::string(&pair[0])?.to_lowercase(), pair[1].clone());
        }
        let query = v::object(
            query
                .into_iter()
                .map(|(key, mut values)| {
                    let value = if key == "algorithm" && verified_algorithm {
                        values.pop().ok_or("verified algorithm absent")?
                    } else {
                        v::array(values)?
                    };
                    Ok((key, value))
                })
                .collect::<Result<Vec<_>>>()?,
        )?;
        let mut fields = vec![
            ("method".into(), v::text(method)),
            ("path".into(), v::text(url.path())),
            ("query".into(), query),
            ("body".into(), v::get(&request, "body_hex")?),
        ];
        for (name, header) in [
            ("user_agent", "user-agent"),
            ("accept", "accept"),
            ("accept_encoding", "accept-encoding"),
            ("connection", "connection"),
            ("length", "content-length"),
            ("cookie", "cookie"),
        ] {
            fields.push((
                name.into(),
                headers.get(header).cloned().unwrap_or_else(v::null),
            ));
        }
        result.push(v::object(fields)?);
    }
    v::array(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn original_response_text_boundaries_preserve_bom_and_explicit_codec_precedence() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/registration/response-text-original.json"
        ))
        .unwrap();
        let rows = fixture["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 150);
        let headers: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/registration/response-text-header-original.json"
        ))
        .unwrap();
        let header_rows = headers["rows"].as_array().unwrap();
        assert_eq!(header_rows.len(), 3);
        for row in rows.iter().chain(header_rows) {
            let body = v::unhex(row["body_hex"].as_str().unwrap()).unwrap();
            let headers = row["headers"]
                .as_array()
                .unwrap()
                .iter()
                .map(|pair| {
                    (
                        pair[0].as_str().unwrap().to_owned(),
                        pair[1].as_str().unwrap().to_owned(),
                    )
                })
                .collect::<Vec<_>>();
            let expected = row["text_codepoints"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| u32::try_from(n.as_u64().unwrap()).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(
                response_text(&body, &headers).unwrap(),
                expected,
                "{} response {}",
                row["case"],
                row["response"]
            );
        }
    }

    #[test]
    fn original_unsigned_algorithm_query_remains_a_list() {
        let fixture = JsonValue::parse(include_str!(
            "../../tests/fixtures/registration/request-normalizer-original.json"
        ))
        .unwrap();
        let actual = normalized(
            &v::array([v::get(&fixture, "raw").unwrap()]).unwrap(),
            &BTreeMap::new(),
            1_700_003_600,
        )
        .unwrap();
        assert!(v::equal(
            &actual,
            &v::array([v::get(&fixture, "expected").unwrap()]).unwrap()
        ));
    }
}
