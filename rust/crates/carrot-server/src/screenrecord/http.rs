use super::{catalog, Failure, Screenrecord};
use crate::{
    http::{Body, RequestBody},
    http_response::{json_response, text},
    Value,
};
use hyper::{
    header::{self, HeaderMap, HeaderValue},
    Method, Request, Response, StatusCode,
};
use num_bigint::BigInt;
use num_traits::ToPrimitive;
use std::{path::Path, sync::Arc};

enum Route<'a> {
    Videos,
    Media(&'static str, &'a str),
}
fn decoded_is(value: &str, literal: &str) -> bool {
    percent_encoding::percent_decode_str(value)
        .decode_utf8()
        .is_ok_and(|value| value == literal)
}
fn route(path: &str) -> Option<Route<'_>> {
    let mut segments = path.strip_prefix('/')?.split('/');
    if !decoded_is(segments.next()?, "api") || !decoded_is(segments.next()?, "screenrecord") {
        return None;
    }
    let kind = segments.next()?;
    if decoded_is(kind, "videos") {
        return segments.next().is_none().then_some(Route::Videos);
    }
    let kind = ["thumbnail", "video", "download"]
        .into_iter()
        .find(|literal| decoded_is(kind, literal))?;
    let id = segments.next().filter(|id| !id.is_empty())?;
    segments.next().is_none().then_some(Route::Media(kind, id))
}
pub fn matches(path: &str) -> bool {
    route(path).is_some()
}
fn failure(error: Failure, head: bool) -> Response<Body> {
    match error {
        Failure::Http { status, message } => text(
            StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            &message,
            head,
        ),
        Failure::Internal => {
            let mut response = text(
                StatusCode::INTERNAL_SERVER_ERROR,
                "500 Internal Server Error\n\nServer got itself in trouble",
                head,
            );
            response
                .headers_mut()
                .insert(header::CONNECTION, HeaderValue::from_static("close"));
            response
                .extensions_mut()
                .insert(hyper::ext::CloseAfterResponse);
            response
        }
    }
}
fn query(request: &Request<RequestBody>, name: &str) -> Option<String> {
    url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes())
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
}
fn mime(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_lowercase)
        .as_deref()
    {
        Some("mkv") => "video/x-matroska",
        Some("ts") => "text/vnd.trolltech.linguist",
        _ => crate::static_assets::content_type(path),
    }
}
fn disposition(path: &Path) -> Result<HeaderValue, Failure> {
    let name = catalog::text(path.file_name().ok_or(Failure::Internal)?);
    let Value::Text(points) = name else {
        return Err(Failure::Internal);
    };
    let name: String = points
        .into_iter()
        .map(|point| {
            if (32..127).contains(&point) && point != 34 && point != 92 {
                char::from_u32(point).unwrap_or('_')
            } else {
                '_'
            }
        })
        .collect();
    HeaderValue::from_str(&format!(
        "attachment; filename=\"{}\"",
        if name.is_empty() {
            "screenrecord"
        } else {
            &name
        }
    ))
    .map_err(|_| Failure::Internal)
}
fn videos(service: &Screenrecord, offset: BigInt, limit: BigInt) -> Result<Value, Failure> {
    let videos = service.cached_videos()?;
    let total = videos.len();
    let begin = offset.to_usize().unwrap_or(total).min(total);
    let end = (offset.clone() + &limit)
        .to_usize()
        .unwrap_or(total)
        .min(total);
    Ok(Value::object([
        ("ok", Value::Bool(true)),
        ("videos", Value::Array(videos[begin..end].to_vec())),
        (
            "folders",
            Value::Array(
                service
                    .directories
                    .iter()
                    .filter(|path| path.is_dir())
                    .map(|path| catalog::text(path.as_os_str()))
                    .collect(),
            ),
        ),
        ("offset", Value::Integer(offset)),
        ("limit", Value::Integer(limit)),
        ("total", Value::integer(total)),
        (
            "nextOffset",
            if end < total {
                Value::integer(end)
            } else {
                Value::Null
            },
        ),
        ("hasMore", Value::Bool(end < total)),
    ]))
}
pub async fn handle(request: Request<RequestBody>, service: Arc<Screenrecord>) -> Response<Body> {
    let head = request.method() == Method::HEAD;
    let Some(selected) = route(request.uri().path()) else {
        return text(StatusCode::NOT_FOUND, "404: Not Found", head);
    };
    if !matches!(request.method(), &Method::GET | &Method::HEAD) {
        let mut response = text(
            StatusCode::METHOD_NOT_ALLOWED,
            "405: Method Not Allowed",
            head,
        );
        response
            .headers_mut()
            .insert(header::ALLOW, HeaderValue::from_static("GET,HEAD"));
        return response;
    }
    let (kind, id) = match selected {
        Route::Videos => {
            let pagination = || -> Result<(BigInt, BigInt), crate::Error> {
                let offset = query(&request, "offset")
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| "0".into());
                let limit = query(&request, "limit")
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| "80".into());
                Ok((
                    Value::text(&offset).int()?.max(BigInt::from(0)),
                    Value::text(&limit)
                        .int()?
                        .clamp(BigInt::from(1), BigInt::from(200)),
                ))
            };
            let result = match pagination() {
                Ok((offset, limit)) => {
                    tokio::task::spawn_blocking(move || videos(&service, offset, limit))
                        .await
                        .map_err(|_| "screenrecord operation failed".to_owned())
                        .and_then(|result| result.map_err(|error| error.to_string()))
                }
                Err(error) => Err(error.to_string()),
            };
            let (status, value) = match result {
                Ok(value) => (StatusCode::OK, value),
                Err(error) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Value::object([("ok", Value::Bool(false)), ("error", Value::text(&error))]),
                ),
            };
            return json_response(status, value, head, "")
                .unwrap_or_else(|_| failure(Failure::Internal, head));
        }
        Route::Media(kind, id) => (kind, id),
    };
    let id = match percent_encoding::percent_decode_str(id).decode_utf8() {
        Ok(id) => id.into_owned(),
        Err(_) => return failure(Failure::Internal, head),
    };
    let kind = kind.to_owned();
    let thumbnail = kind == "thumbnail";
    let path = tokio::task::spawn_blocking(move || {
        if thumbnail {
            service.thumbnail(&id)
        } else {
            service.find_file(&id)
        }
    })
    .await;
    let path = match path {
        Ok(Ok(path)) => path,
        Ok(Err(error)) => return failure(error, head),
        Err(_) => return failure(Failure::Internal, head),
    };
    let mut headers = HeaderMap::new();
    if thumbnail {
        headers.insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=86400"),
        );
    } else {
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(mime(&path)));
        if kind == "video" {
            headers.insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("private, max-age=3600"),
            );
        }
        if kind == "download" || query(&request, "download").is_some_and(|value| !value.is_empty())
        {
            match disposition(&path) {
                Ok(value) => {
                    headers.insert(header::CONTENT_DISPOSITION, value);
                }
                Err(error) => return failure(error, head),
            }
        }
    }
    crate::static_web::file_response_with_headers(&path, &request, headers)
        .await
        .unwrap_or_else(|_| failure(Failure::Internal, head))
}
