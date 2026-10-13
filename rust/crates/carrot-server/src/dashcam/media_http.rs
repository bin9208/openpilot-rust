use super::{media::Media, Failure};
use crate::{
    http::{Body, RequestBody},
    http_response::text,
    Value,
};
use hyper::{
    header::{self, HeaderMap, HeaderValue},
    Method, Request, Response, StatusCode,
};
use std::sync::Arc;

fn decoded_is(raw: &str, text: &str) -> bool {
    percent_encoding::percent_decode_str(raw)
        .decode_utf8()
        .is_ok_and(|value| value == text)
}
fn route(path: &str) -> Option<(&'static str, &str)> {
    let mut parts = path.strip_prefix('/')?.split('/');
    if !decoded_is(parts.next()?, "api") || !decoded_is(parts.next()?, "dashcam") {
        return None;
    }
    let kind = parts.next()?;
    let kind = ["thumbnail", "preview", "video"]
        .into_iter()
        .find(|name| decoded_is(kind, name))?;
    let segment = parts.next().filter(|value| !value.is_empty())?;
    parts.next().is_none().then_some((kind, segment))
}
pub fn matches(path: &str) -> bool {
    route(path).is_some()
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
pub async fn handle(request: Request<RequestBody>, media: Arc<Media>) -> Response<Body> {
    let head = request.method() == Method::HEAD;
    let Some((kind, raw)) = route(request.uri().path()) else {
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
    let segment = percent_encoding::percent_decode_str(raw)
        .decode_utf8_lossy()
        .into_owned();
    let download = url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes())
        .find(|(key, _)| key == "download")
        .is_some_and(|(_, value)| !value.is_empty());
    let worker_segment = Value::text(&segment);
    let result = tokio::task::spawn_blocking(move || match kind {
        "thumbnail" => media.thumbnail(&worker_segment).map(|path| (path, None)),
        "preview" => media.preview(&worker_segment).map(|path| (path, None)),
        _ => media
            .video(&worker_segment)
            .map(|(path, content_type)| (path, Some(content_type))),
    })
    .await;
    let (path, content_type) = match result {
        Ok(Ok(value)) => value,
        Ok(Err(Failure::Http { status, message })) => {
            return text(
                StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                &message,
                head,
            )
        }
        Ok(Err(Failure::InvalidRecent | Failure::Runtime(_))) | Err(_) => return failed(head),
    };
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(if kind == "video" {
            "private, max-age=3600"
        } else {
            "public, max-age=86400"
        }),
    );
    if let Some(content_type) = content_type {
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    }
    if kind == "video" && download {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .map_or_else(|| ".mp4".into(), |value| format!(".{value}"));
        let value = format!("attachment; filename=\"{segment}{extension}\"");
        let Ok(value) = HeaderValue::from_str(&value) else {
            return failed(head);
        };
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    crate::static_web::file_response_with_headers(&path, &request, headers)
        .await
        .unwrap_or_else(|_| failed(head))
}
