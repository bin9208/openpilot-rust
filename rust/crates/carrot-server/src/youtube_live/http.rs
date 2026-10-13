use super::service::Service;
use crate::{
    http::{Body, RequestBody},
    http_response::{json_response, text},
    Error, Value,
};
use hyper::{header, Method, Request, Response, StatusCode};
use std::sync::Arc;

pub fn matches(path: &str) -> bool {
    matches!(
        path,
        "/api/youtube_live/status"
            | "/api/youtube_live/diagnostics"
            | "/api/youtube_live/stream_key"
            | "/api/youtube_live/stream_key/validate"
            | "/api/youtube_live/test"
    )
}
fn response(status: StatusCode, value: Value, head: bool) -> Response<Body> {
    json_response(status, value, head, "").unwrap_or_else(|_| internal(head))
}
fn success(value: Value, head: bool) -> Response<Body> {
    let mut output = Value::object([("ok", Value::Bool(true))]);
    if let (Value::Object(target), Value::Object(fields)) = (&mut output, value) {
        target.extend(fields);
    }
    response(StatusCode::OK, output, head)
}
fn internal(head: bool) -> Response<Body> {
    let mut result = text(
        StatusCode::INTERNAL_SERVER_ERROR,
        "500 Internal Server Error\n\nServer got itself in trouble",
        head,
    );
    result.headers_mut().insert(
        header::CONNECTION,
        header::HeaderValue::from_static("close"),
    );
    result
        .extensions_mut()
        .insert(hyper::ext::CloseAfterResponse);
    result
}
fn error(error: Error, head: bool) -> Response<Body> {
    if error.to_string() == "youtube live service unavailable" {
        text(
            StatusCode::SERVICE_UNAVAILABLE,
            "youtube live service unavailable",
            head,
        )
    } else {
        internal(head)
    }
}
fn invalid_json(head: bool) -> Response<Body> {
    response(
        StatusCode::BAD_REQUEST,
        Value::object([
            ("ok", Value::Bool(false)),
            ("error", Value::text("invalid json")),
        ]),
        head,
    )
}
pub async fn handle(
    request: Request<RequestBody>,
    service: Option<Arc<Service>>,
) -> Response<Body> {
    let head = request.method() == Method::HEAD;
    let path = percent_encoding::percent_decode_str(request.uri().path())
        .decode_utf8_lossy()
        .into_owned();
    let method = request.method().clone();
    let key = path == "/api/youtube_live/stream_key";
    let read = matches!(
        path.as_str(),
        "/api/youtube_live/status" | "/api/youtube_live/diagnostics"
    );
    let allowed = if key {
        matches!(
            method,
            Method::GET | Method::HEAD | Method::POST | Method::DELETE
        )
    } else if read {
        matches!(method, Method::GET | Method::HEAD)
    } else {
        method == Method::POST
    };
    if !matches(&path) {
        return text(StatusCode::NOT_FOUND, "404: Not Found", head);
    }
    if !allowed {
        let mut result = text(
            StatusCode::METHOD_NOT_ALLOWED,
            "405: Method Not Allowed",
            head,
        );
        result.headers_mut().insert(
            header::ALLOW,
            header::HeaderValue::from_static(if key {
                "DELETE,GET,HEAD,POST"
            } else if read {
                "GET,HEAD"
            } else {
                "POST"
            }),
        );
        return result;
    }
    let Some(service) = service else {
        return text(
            StatusCode::SERVICE_UNAVAILABLE,
            "youtube live service unavailable",
            head,
        );
    };
    if key && matches!(method, Method::GET | Method::HEAD) {
        return match service.get_key().await {
            Ok(key) => response(
                StatusCode::OK,
                Value::object([
                    ("ok", Value::Bool(true)),
                    ("configured", Value::Bool(key.truth())),
                    ("stream_key", key),
                ]),
                head,
            ),
            Err(failure) => error(failure, head),
        };
    }
    if key && method == Method::DELETE {
        return match service.clear_key().await {
            Ok(value) => success(value, head),
            Err(failure) => error(failure, head),
        };
    }
    if key && method == Method::POST {
        let body = match crate::http_request::read_json(request).await {
            Ok(value) => value,
            Err(_) => return invalid_json(head),
        };
        if !matches!(body, Value::Object(_)) {
            return internal(head);
        }
        let value = if body.has("stream_key") {
            body.get("stream_key")
        } else {
            body.get("key")
        };
        let value = if value.truth() {
            value.py_string().unwrap_or_else(|_| Value::text(""))
        } else {
            Value::text("")
        };
        return match service.set_key(value).await {
            Ok(value) => success(value, head),
            Err(failure) if failure.to_string() == "stream key is required" => response(
                StatusCode::BAD_REQUEST,
                Value::object([
                    ("ok", Value::Bool(false)),
                    ("error", Value::text("stream key is required")),
                ]),
                head,
            ),
            Err(failure) => error(failure, head),
        };
    }
    if path == "/api/youtube_live/stream_key/validate" {
        let can_read = request
            .headers()
            .get(header::CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value != "0")
            || request.headers().contains_key(header::TRANSFER_ENCODING);
        let body = if can_read {
            match crate::http_request::read_json(request).await {
                Ok(value) => value,
                Err(_) => return invalid_json(head),
            }
        } else {
            Value::object([])
        };
        let value = matches!(body, Value::Object(_)).then(|| {
            if body.has("stream_key") {
                body.get("stream_key").clone()
            } else {
                body.get("key").clone()
            }
        });
        return match service.verify(value).await {
            Ok(value) => response(
                if value.get("ok").truth() {
                    StatusCode::OK
                } else {
                    StatusCode::CONFLICT
                },
                value,
                head,
            ),
            Err(failure) => error(failure, head),
        };
    }
    if path == "/api/youtube_live/test" {
        return match service.test().await {
            Ok(value) => response(
                if value.get("ok").truth() {
                    StatusCode::OK
                } else {
                    StatusCode::CONFLICT
                },
                value,
                head,
            ),
            Err(failure) => error(failure, head),
        };
    }
    if path == "/api/youtube_live/diagnostics" {
        return match service.diagnostics().await {
            Ok(value) => response(
                StatusCode::OK,
                Value::object([("ok", Value::Bool(true)), ("diagnostics", value)]),
                head,
            ),
            Err(failure) => error(failure, head),
        };
    }
    match service.status().await {
        Ok(value) => success(value, head),
        Err(failure) => error(failure, head),
    }
}
