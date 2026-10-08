use super::{bad, status::status, Failure, Service, Value};
use crate::{
    http::{Body, RequestBody},
    http_response::{json_response, text},
    Error,
};
use futures_util::FutureExt;
use http_body_util::BodyExt;
use hyper::{header, Method, Request, Response, StatusCode};
use std::sync::Arc;

pub fn matches(path: &str) -> bool {
    path == "/api/bluetooth"
        || path
            .strip_prefix("/api/bluetooth/")
            .is_some_and(|operation| !operation.is_empty() && !operation.contains('/'))
}

pub(super) fn guard<T>(request: &Request<T>, service: &Service) -> Result<(), Failure> {
    let header = |name| {
        request
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
    };
    let origin = header("origin");
    if (!origin.is_empty() && crate::xiaoge::origin_netloc(origin)? != header("host"))
        || header("sec-fetch-site") == "cross-site"
    {
        return Err(Failure::Http(
            StatusCode::FORBIDDEN,
            "same-origin requests only".into(),
        ));
    }
    if !header("content-type")
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .eq_ignore_ascii_case("application/json")
    {
        return Err(Failure::Http(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "application/json required".into(),
        ));
    }
    if !service.runtime()?.get("stationary").truth() {
        return Err(Failure::Http(
            StatusCode::CONFLICT,
            "setup requires fresh stationary and disengaged state".into(),
        ));
    }
    Ok(())
}

fn too_large(size: usize) -> Failure {
    Failure::Http(
        StatusCode::PAYLOAD_TOO_LARGE,
        format!("Maximum request body size 32768 exceeded, actual body size {size}"),
    )
}

async fn read_once(mut body: RequestBody) -> Result<Vec<u8>, Failure> {
    let mut bytes = Vec::new();
    while let Some(frame) = if bytes.is_empty() {
        body.frame().await
    } else {
        body.frame().now_or_never().flatten()
    } {
        let frame = frame.map_err(Error::Request)?;
        if let Ok(data) = frame.into_data() {
            let count = data.len().min(32769 - bytes.len());
            bytes.extend_from_slice(&data[..count]);
            if bytes.len() == 32769 {
                return Err(too_large(bytes.len()));
            }
        }
    }
    Ok(bytes)
}

fn parse(bytes: &[u8]) -> Result<Value, Failure> {
    let encoding =
        if bytes.starts_with(&[0, 0, 0xfe, 0xff]) || bytes.starts_with(&[0xff, 0xfe, 0, 0]) {
            "utf32"
        } else if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
            "utf16"
        } else if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
            "utf8_sig"
        } else if bytes.len() >= 4 && bytes[0] == 0 && bytes[1] == 0 && bytes[2] == 0 {
            "utf32be"
        } else if bytes.len() >= 4 && bytes[1] == 0 && bytes[2] == 0 && bytes[3] == 0 {
            "utf32le"
        } else if bytes.len() >= 2 && bytes[0] == 0 {
            "utf16be"
        } else if bytes.len() >= 2 && bytes[1] == 0 {
            "utf16le"
        } else {
            "utf8"
        };
    let decoded =
        crate::request_text::decode(bytes, encoding).map_err(|error| bad(&error.to_string()))?;
    Ok(Value::parse(&decoded)?)
}

fn failure(error: Failure, head: bool, mutation: bool) -> Response<Body> {
    let status = match &error {
        Failure::Http(status, _) => *status,
        _ if !mutation => StatusCode::INTERNAL_SERVER_ERROR,
        Failure::Config(_) => StatusCode::BAD_REQUEST,
        Failure::Json(error)
            if matches!(
                error.kind,
                "ValueError" | "JSONDecodeError" | "TypeError" | "KeyError"
            ) =>
        {
            StatusCode::BAD_REQUEST
        }
        Failure::Bluez(
            openpilot_bluetooth::bluez::Error::Policy(_)
            | openpilot_bluetooth::bluez::Error::Device
            | openpilot_bluetooth::bluez::Error::Adapter
            | openpilot_bluetooth::bluez::Error::Pairing
            | openpilot_bluetooth::bluez::Error::PromptExpired,
        ) => StatusCode::BAD_REQUEST,
        _ if mutation => StatusCode::BAD_GATEWAY,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    if let Failure::Source(error) = &error {
        if let Some(response) = crate::http_response::parser_response(error, head) {
            return response;
        }
    }
    let message = if status == StatusCode::INTERNAL_SERVER_ERROR {
        "500 Internal Server Error\n\nServer got itself in trouble".into()
    } else {
        error.to_string()
    };
    let mut response = text(status, &message, head);
    if status == StatusCode::INTERNAL_SERVER_ERROR {
        response.headers_mut().insert(
            header::CONNECTION,
            header::HeaderValue::from_static("close"),
        );
    }
    response
}

pub async fn handle(request: Request<RequestBody>, service: Arc<Service>) -> Response<Body> {
    let head = request.method() == Method::HEAD;
    let path = percent_encoding::percent_decode_str(request.uri().path())
        .decode_utf8_lossy()
        .into_owned();
    if !matches(&path) {
        return text(StatusCode::NOT_FOUND, "404: Not Found", head);
    }
    let is_status = path == "/api/bluetooth";
    let methods = if is_status { "GET,HEAD" } else { "POST" };
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
    let result = if is_status {
        status(&service).await
    } else {
        if let Err(error) = guard(&request, &service) {
            return failure(error, head, false);
        }
        if let Some(size) = request
            .headers()
            .get(header::CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<usize>().ok())
        {
            if size > 32768 {
                return failure(too_large(size), head, true);
            }
        }
        let (parts, body) = request.into_parts();
        let bytes = match read_once(body).await {
            Ok(bytes) => bytes,
            Err(error) => return failure(error, head, false),
        };
        let body = match parse(&bytes) {
            Ok(body) => body,
            Err(error) => return failure(error, head, true),
        };
        let request = Request::from_parts(parts, ());
        service
            .mutate(&request, &path[15..], body)
            .await
            .map(|()| Value::object([("ok", Value::Bool(true))]))
    };
    match result {
        Ok(value) => json_response(StatusCode::OK, value, head, "")
            .unwrap_or_else(|error| failure(error.into(), head, false)),
        Err(error) => failure(error, head, !is_status),
    }
}
