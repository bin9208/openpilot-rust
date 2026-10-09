use super::{Online, ProxyError, MAX_REQUEST_BYTES};
use crate::{
    http::{Body, RequestBody},
    http_response::{json_response, parser_response, response, text},
    Error, Value,
};
use http_body_util::BodyExt;
use hyper::{header, Method, Request, Response, StatusCode};
use std::{path::Path, sync::Arc};

/// Matches the original diagnostic routes using a decoded request path.
pub fn matches(path: &str) -> bool {
    allowed(path).is_some()
}

fn allowed(path: &str) -> Option<&'static str> {
    match path {
        "/xiaoge" | "/xiaoge/" => Some("GET,HEAD"),
        "/xiaoge/api/status" | "/xiaoge/api/snapshot" => Some("GET"),
        "/xiaoge/api/config" => Some("DELETE,GET,POST"),
        "/xiaoge/api/settings" => Some("POST"),
        _ => None,
    }
}

fn no_store(mut result: Response<Body>) -> Response<Body> {
    result.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    result
}

fn unavailable(error: ProxyError) -> Response<Body> {
    let status = match error {
        ProxyError::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
        ProxyError::Timeout => StatusCode::GATEWAY_TIMEOUT,
        ProxyError::BadResponse => StatusCode::BAD_GATEWAY,
    };
    let code = error.to_string();
    no_store(
        json_response(
            status,
            Value::object([("error", Value::text(&code)), ("code", Value::text(&code))]),
            false,
            "",
        )
        .unwrap_or_else(|error| internal(error, false)),
    )
}

fn internal(error: Error, head: bool) -> Response<Body> {
    parser_response(&error, head).unwrap_or_else(|| {
        eprintln!("xiaoge request: {error}");
        text(
            StatusCode::INTERNAL_SERVER_ERROR,
            "500 Internal Server Error\n\nServer got itself in trouble",
            head,
        )
    })
}

pub(crate) fn origin_netloc(origin: &str) -> Result<String, Error> {
    let origin = origin
        .trim_start_matches(|point| point <= '\u{20}')
        .replace(['\t', '\r', '\n'], "");
    let remainder = origin
        .split_once(':')
        .map_or(origin.as_str(), |(scheme, rest)| {
            if scheme.starts_with(|point: char| point.is_ascii_alphabetic())
                && scheme
                    .chars()
                    .all(|point| point.is_ascii_alphanumeric() || "+-.".contains(point))
            {
                rest
            } else {
                origin.as_str()
            }
        });
    let Some(authority) = remainder.strip_prefix("//") else {
        return Ok(String::new());
    };
    let netloc = authority.split(['/', '?', '#']).next().unwrap_or("");
    if netloc.contains(['[', ']']) {
        url::Url::parse(&format!("http://{netloc}"))
            .map_err(|error| Error::Source(error.to_string()))?;
    }
    Ok(netloc.to_owned())
}

enum BodyFailure {
    Payload(crate::DecodeFailure),
    TooLarge(usize),
}

async fn read_body(request: Request<RequestBody>) -> Result<Vec<u8>, BodyFailure> {
    let mut body = request.into_body();
    let mut bytes = Vec::new();
    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(BodyFailure::Payload)?;
        if let Ok(data) = frame.into_data() {
            let size = bytes.len().saturating_add(data.len());
            if size > MAX_REQUEST_BYTES {
                return Err(BodyFailure::TooLarge(size));
            }
            bytes.extend_from_slice(&data);
        }
    }
    Ok(bytes)
}

/// Serves the original page and bounded diagnostic proxy through the supplied provider.
pub async fn handle(
    request: Request<RequestBody>,
    repository: &Path,
    online: Arc<Online>,
) -> Response<Body> {
    let head = request.method() == Method::HEAD;
    let path = percent_encoding::percent_decode_str(request.uri().path())
        .decode_utf8_lossy()
        .into_owned();
    let Some(methods) = allowed(&path) else {
        return text(StatusCode::NOT_FOUND, "404: Not Found", head);
    };
    if !methods
        .split(',')
        .any(|method| request.method().as_str() == method)
    {
        let mut result = text(
            StatusCode::METHOD_NOT_ALLOWED,
            "405: Method Not Allowed",
            head,
        );
        result
            .headers_mut()
            .insert(header::ALLOW, header::HeaderValue::from_static(methods));
        return result;
    }
    if path == "/xiaoge" {
        let mut result = text(StatusCode::FOUND, "302: Found", head);
        let location = format!(
            "/xiaoge/{}",
            request
                .uri()
                .query()
                .map_or(String::new(), |query| format!("?{query}"))
        );
        return match location.parse() {
            Ok(location) => {
                result.headers_mut().insert(header::LOCATION, location);
                result
            }
            Err(error) => internal(Error::Source(format!("invalid redirect: {error}")), head),
        };
    }
    if path == "/xiaoge/" {
        let page = repository.join("openpilot/selfdrive/carrot/xiaoge/v_asm_web.html");
        return no_store(
            match crate::static_web::file_response(&page, &request).await {
                Ok(result) => result,
                Err(error) => internal(error, head),
            },
        );
    }
    let endpoint = path.rsplit('/').next().unwrap_or("");
    let stream = if endpoint == "snapshot" {
        let stream = url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes())
            .find_map(|(key, value)| (key == "stream").then(|| value.into_owned()))
            .unwrap_or_else(|| "wide".into());
        if !matches!(stream.as_str(), "wide" | "road") {
            return text(
                StatusCode::BAD_REQUEST,
                "stream must be wide or road",
                false,
            );
        }
        Some(stream)
    } else {
        None
    };
    let method = request.method().clone();
    if matches!(method, Method::POST | Method::DELETE) {
        if let Some(origin) = request
            .headers()
            .get(header::ORIGIN)
            .filter(|value| !value.is_empty())
        {
            let origin = String::from_utf8_lossy(origin.as_bytes());
            let netloc = match origin_netloc(&origin) {
                Ok(netloc) => netloc,
                Err(error) => return internal(error, false),
            };
            let host = request
                .headers()
                .get(header::HOST)
                .map(header::HeaderValue::as_bytes)
                .unwrap_or_default();
            if netloc.as_bytes() != host {
                return text(
                    StatusCode::FORBIDDEN,
                    "cross-origin diagnostic changes are not allowed",
                    false,
                );
            }
        }
    }
    let body = if method == Method::POST {
        let content_type = request
            .headers()
            .get(header::CONTENT_TYPE)
            .map(|value| String::from_utf8_lossy(value.as_bytes()));
        if !content_type.is_some_and(|value| {
            value
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .eq_ignore_ascii_case("application/json")
        }) {
            return text(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "expected application/json",
                false,
            );
        }
        match read_body(request).await {
            Ok(body) => Some(body),
            Err(BodyFailure::Payload(error)) => return internal(error.into(), false),
            Err(BodyFailure::TooLarge(size)) => {
                return text(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    &format!(
                        "Maximum request body size {} exceeded, actual body size {size}",
                        MAX_REQUEST_BYTES + 1
                    ),
                    false,
                );
            }
        }
    } else {
        None
    };
    let endpoint = endpoint.to_owned();
    match tokio::task::spawn_blocking(move || {
        online.fetch(method, &endpoint, stream.as_deref(), body.as_deref())
    })
    .await
    {
        Ok(Ok(upstream)) => {
            let mut result = response(
                upstream.status,
                upstream.bytes,
                "application/octet-stream",
                false,
            );
            if let Some(content_type) = upstream.content_type {
                result
                    .headers_mut()
                    .insert(header::CONTENT_TYPE, content_type);
            }
            if upstream.status == StatusCode::NO_CONTENT {
                result.headers_mut().remove(header::CONTENT_LENGTH);
            }
            no_store(result)
        }
        Ok(Err(error)) => unavailable(error),
        Err(error) => internal(Error::Source(error.to_string()), false),
    }
}
