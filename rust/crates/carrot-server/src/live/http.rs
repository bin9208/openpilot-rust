pub use super::routes::matches;
use super::{routes::parsed, Service};
use crate::web_sound_http::handshake;
use crate::{
    http::{Body, RequestBody},
    http_response::{json_response, text},
    Value,
};
use bytes::Bytes;
use http_body_util::Full;
use hyper::{header, Method, Request, Response, StatusCode};
use std::sync::Arc;
use tokio_tungstenite::tungstenite::handshake::derive_accept_key;

pub async fn handle(
    mut request: Request<RequestBody>,
    service: Option<Arc<Service>>,
    unavailable: Option<&str>,
) -> Response<Body> {
    let head = request.method() == Method::HEAD;
    let path = percent_encoding::percent_decode_str(request.uri().path())
        .decode_utf8_lossy()
        .into_owned();
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
    if path == "/api/live_runtime" {
        let force = url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes())
            .find(|(name, _)| name == "force")
            .is_some_and(|(_, value)| value == "1");
        let (status, payload) = match service {
            Some(service) => match service.snapshot(force).await {
                Ok(payload) => (
                    if payload.get("ok").truth() {
                        StatusCode::OK
                    } else {
                        StatusCode::SERVICE_UNAVAILABLE
                    },
                    payload,
                ),
                Err(error) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Value::object([
                        ("ok", Value::Bool(false)),
                        ("error", Value::text(&error.to_string())),
                    ]),
                ),
            },
            None => (
                StatusCode::SERVICE_UNAVAILABLE,
                Value::object([
                    ("ok", Value::Bool(false)),
                    (
                        "error",
                        Value::text(unavailable.unwrap_or("realtime broker unavailable")),
                    ),
                ]),
            ),
        };
        return json_response(status, payload, head, "")
            .unwrap_or_else(|error| crate::http_response::error_response(error.to_string(), head));
    }
    let Some(service) = service else {
        return text(
            StatusCode::SERVICE_UNAVAILABLE,
            if path.starts_with("/ws/camera/") {
                "realtime camera hub unavailable"
            } else if path == "/ws/compact_state" {
                "realtime state hub unavailable"
            } else {
                "realtime raw hub unavailable"
            },
            head,
        );
    };
    let spec = match parsed(&request, &path) {
        Ok(spec) => spec,
        Err((status, message)) => return text(status, &message, head),
    };
    if let Some(error) = handshake::handshake_error(request.headers()) {
        return text(StatusCode::BAD_REQUEST, &error, head);
    }
    let Some(key) = request.headers().get(header::SEC_WEBSOCKET_KEY) else {
        return text(StatusCode::BAD_REQUEST, "Handshake error: None", head);
    };
    let accept = match header::HeaderValue::try_from(derive_accept_key(key.as_bytes())) {
        Ok(accept) => accept,
        Err(error) => return crate::http_response::error_response(error.to_string(), head),
    };
    if let Err(error) = service.launch(hyper::upgrade::on(&mut request), spec).await {
        return text(StatusCode::SERVICE_UNAVAILABLE, &error.to_string(), head);
    }
    let mut response = Response::new(Full::new(Bytes::new()));
    *response.status_mut() = StatusCode::SWITCHING_PROTOCOLS;
    response.headers_mut().insert(
        header::UPGRADE,
        header::HeaderValue::from_static("websocket"),
    );
    response.headers_mut().insert(
        header::CONNECTION,
        header::HeaderValue::from_static("upgrade"),
    );
    response
        .headers_mut()
        .insert(header::SEC_WEBSOCKET_ACCEPT, accept);
    response
}
