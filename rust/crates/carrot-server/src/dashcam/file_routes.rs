use super::{metadata, mime::DownloadMime, raw_files, Failure, Service};
use crate::{
    http::{Body, RequestBody},
    http_response::{json_response, text},
    Value,
};
use hyper::{
    header::{self, HeaderValue},
    Method, Request, Response, StatusCode,
};
use std::{path::PathBuf, sync::Arc};

pub struct MetadataFiles {
    service: Arc<Service>,
    mime: DownloadMime,
}
impl MetadataFiles {
    pub fn original(service: Arc<Service>) -> Arc<Self> {
        Arc::new(Self {
            service,
            mime: DownloadMime::original(),
        })
    }
    pub fn for_test(service: Arc<Service>, mime_files: Vec<PathBuf>) -> Arc<Self> {
        Arc::new(Self {
            service,
            mime: DownloadMime::new(mime_files),
        })
    }
}
enum Route<'a> {
    Summary(&'a str),
    Replay(&'a str),
    ReplayFile(&'a str, &'a str),
    Download(&'a str, &'a str),
}
fn decoded_is(value: &str, literal: &str) -> bool {
    percent_encoding::percent_decode_str(value)
        .decode_utf8()
        .is_ok_and(|value| value == literal)
}
fn route(path: &str) -> Option<Route<'_>> {
    let mut parts = path.strip_prefix('/')?.split('/');
    if !decoded_is(parts.next()?, "api") || !decoded_is(parts.next()?, "dashcam") {
        return None;
    }
    let kind = parts.next()?;
    let id = parts.next().filter(|value| !value.is_empty())?;
    let suffix = parts.next();
    if parts.next().is_some() {
        return None;
    }
    if decoded_is(kind, "summary-source") && suffix.is_none() {
        Some(Route::Summary(id))
    } else if decoded_is(kind, "replay-source") {
        match suffix {
            None => Some(Route::Replay(id)),
            Some(kind) if !kind.is_empty() => Some(Route::ReplayFile(id, kind)),
            Some(_) => None,
        }
    } else if decoded_is(kind, "download") {
        suffix
            .filter(|value| !value.is_empty())
            .map(|kind| Route::Download(id, kind))
    } else {
        None
    }
}
pub fn matches(path: &str) -> bool {
    route(path).is_some()
}
fn decoded(value: &str) -> String {
    percent_encoding::percent_decode_str(value)
        .decode_utf8_lossy()
        .into_owned()
}
fn stripped(value: &str) -> String {
    let points = value.chars().map(u32::from).collect::<Vec<_>>();
    crate::state::trim(&points)
        .iter()
        .filter_map(|point| char::from_u32(*point))
        .collect()
}
fn failed(head: bool) -> Response<Body> {
    let mut result = text(
        StatusCode::INTERNAL_SERVER_ERROR,
        "500 Internal Server Error\n\nServer got itself in trouble",
        head,
    );
    result
        .headers_mut()
        .insert(header::CONNECTION, HeaderValue::from_static("close"));
    result
        .extensions_mut()
        .insert(hyper::ext::CloseAfterResponse);
    result
}
fn reply(status: u16, value: Value, head: bool, no_store: bool) -> Response<Body> {
    let mut result = json_response(
        StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        value,
        head,
        "",
    )
    .unwrap_or_else(|_| failed(head));
    if no_store {
        result
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    result
}
fn rejected(status: u16, error: &str, head: bool, no_store: bool) -> Response<Body> {
    reply(
        status,
        Value::object([("ok", Value::Bool(false)), ("error", Value::text(error))]),
        head,
        no_store,
    )
}
async fn file(
    request: Request<RequestBody>,
    result: Result<(PathBuf, hyper::HeaderMap), Failure>,
) -> Response<Body> {
    let head = request.method() == Method::HEAD;
    match result {
        Ok((path, headers)) => {
            crate::static_web::file_response_with_headers(&path, &request, headers)
                .await
                .unwrap_or_else(|_| failed(head))
        }
        Err(Failure::Http { status, message }) => text(
            StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            &message,
            head,
        ),
        Err(Failure::InvalidRecent | Failure::Runtime(_)) => failed(head),
    }
}
pub async fn handle(request: Request<RequestBody>, files: Arc<MetadataFiles>) -> Response<Body> {
    let head = request.method() == Method::HEAD;
    let Some(route) = route(request.uri().path()) else {
        return text(StatusCode::NOT_FOUND, "404: Not Found", head);
    };
    if !matches!(request.method(), &Method::GET | &Method::HEAD) {
        let mut result = text(
            StatusCode::METHOD_NOT_ALLOWED,
            "405: Method Not Allowed",
            head,
        );
        result
            .headers_mut()
            .insert(header::ALLOW, HeaderValue::from_static("GET,HEAD"));
        return result;
    }
    match route {
        Route::Summary(raw) => {
            let route = decoded(raw);
            let result =
                tokio::task::spawn_blocking(move || metadata::summary(&files.service, &route))
                    .await;
            match result {
                Ok(Ok(Some(value))) => reply(200, value, head, true),
                Ok(Ok(None)) => rejected(404, "route not found", head, false),
                Ok(Err(Failure::Http { status, message })) => {
                    rejected(status, &message, head, false)
                }
                Ok(Err(Failure::InvalidRecent | Failure::Runtime(_))) | Err(_) => {
                    rejected(500, "summary source unavailable", head, false)
                }
            }
        }
        Route::Replay(raw) => {
            let segment = Value::text(&decoded(raw));
            match metadata::replay(&files.service, &segment) {
                Ok(value) => reply(200, value, head, true),
                Err(Failure::Http { status, message }) => rejected(status, &message, head, true),
                Err(error @ (Failure::InvalidRecent | Failure::Runtime(_))) => {
                    rejected(500, &error.to_string(), head, true)
                }
            }
        }
        Route::ReplayFile(raw, kind) => {
            let segment = Value::text(&decoded(raw));
            let kind = stripped(&decoded(kind));
            let result = raw_files::replay(&files.service, &segment, &kind);
            file(request, result).await
        }
        Route::Download(raw, kind) => {
            let segment = Value::text(&decoded(raw));
            let kind = stripped(&decoded(kind));
            let result = raw_files::download(&files.service, &files.mime, &segment, &kind);
            file(request, result).await
        }
    }
}
