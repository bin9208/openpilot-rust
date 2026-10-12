use super::{report, Service};
use crate::{
    http::{Body, RequestBody},
    http_response::{json_response, text},
    Value,
};
use hyper::{header, Method, Request, Response, StatusCode};
use std::sync::Arc;

fn route(path: &str) -> Option<&str> {
    let mut parts = path.strip_prefix('/')?.split('/');
    for name in ["api", "dashcam", "report"] {
        let part = parts.next()?;
        if !percent_encoding::percent_decode_str(part)
            .decode_utf8()
            .is_ok_and(|part| part == name)
        {
            return None;
        }
    }
    let name = parts.next().filter(|name| !name.is_empty())?;
    parts.next().is_none().then_some(name)
}
pub fn matches(path: &str) -> bool {
    route(path).is_some()
}
pub async fn handle(request: Request<RequestBody>, service: Arc<Service>) -> Response<Body> {
    let head = request.method() == Method::HEAD;
    let Some(raw) = route(request.uri().path()) else {
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
            .insert(header::ALLOW, header::HeaderValue::from_static("GET,HEAD"));
        return response;
    }
    let name = percent_encoding::percent_decode_str(raw)
        .decode_utf8_lossy()
        .into_owned();
    let prefer = url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes())
        .find(|(key, _)| key == "source")
        .is_none_or(|(_, value)| value != "qlog");
    let result = tokio::task::spawn_blocking(move || {
        report::build(&service.root, &Value::text(&name), prefer)
    })
    .await;
    let (status, payload) = match result {
        Ok(Ok(payload)) => (
            if payload.get("ok").truth() {
                StatusCode::OK
            } else {
                StatusCode::NOT_FOUND
            },
            payload,
        ),
        Ok(Err(error)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Value::object([
                ("ok", Value::Bool(false)),
                ("error", Value::text(&error.to_string())),
            ]),
        ),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Value::object([
                ("ok", Value::Bool(false)),
                ("error", Value::text(&error.to_string())),
            ]),
        ),
    };
    json_response(status, payload, head, "")
        .unwrap_or_else(|error| text(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string(), head))
}
