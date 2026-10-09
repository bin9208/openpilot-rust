use super::{
    session::{Mode, Spec},
    Service,
};
use crate::{
    http::{Body, RequestBody},
    http_response::{json_response, text},
    web_sound_http::handshake,
    Value,
};
use bytes::Bytes;
use http_body_util::Full;
use hyper::{header, Method, Request, Response, StatusCode};
use std::sync::Arc;
use tokio_tungstenite::tungstenite::handshake::derive_accept_key;

pub fn matches(path: &str) -> bool {
    matches!(
        path,
        "/api/terminal_pty/status"
            | "/ws/terminal_pty"
            | "/ws/terminal"
            | "/api/terminal_commands"
            | "/api/terminal_commands/run"
            | "/download/tmux.log"
    )
}
fn query(request: &Request<RequestBody>, name: &str) -> String {
    url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes())
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
        .unwrap_or_default()
}
pub async fn handle(mut request: Request<RequestBody>, service: Arc<Service>) -> Response<Body> {
    let head = request.method() == Method::HEAD;
    let path = percent_encoding::percent_decode_str(request.uri().path()).decode_utf8_lossy();
    let run = path == "/api/terminal_commands/run";
    if if run {
        request.method() != Method::POST
    } else {
        !matches!(request.method(), &Method::GET | &Method::HEAD)
    } {
        let mut response = text(
            StatusCode::METHOD_NOT_ALLOWED,
            "405: Method Not Allowed",
            head,
        );
        response.headers_mut().insert(
            header::ALLOW,
            header::HeaderValue::from_static(if run { "POST" } else { "GET,HEAD" }),
        );
        return response;
    }
    if run {
        return super::cli_http::run(request, service).await;
    }
    if path == "/api/terminal_commands" {
        return json_response(
            StatusCode::OK,
            crate::terminal_commands::registry::listing(),
            head,
            "",
        )
        .unwrap_or_else(|error| crate::http_response::error_response(error.to_string(), head));
    }
    if path == "/download/tmux.log" {
        if !service.config.tmux_log.exists() {
            return json_response(
                StatusCode::NOT_FOUND,
                Value::object([
                    ("ok", Value::Bool(false)),
                    ("error", Value::text("file not found")),
                ]),
                head,
                "",
            )
            .unwrap_or_else(|error| crate::http_response::error_response(error.to_string(), head));
        }
        let mut preset = hyper::HeaderMap::new();
        preset.insert(
            header::CONTENT_DISPOSITION,
            header::HeaderValue::from_static("attachment; filename=tmux.log"),
        );
        return crate::static_web::file_response_with_headers(
            &service.config.tmux_log,
            &request,
            preset,
        )
        .await
        .unwrap_or_else(|error| crate::http_response::error_response(error.to_string(), head));
    }
    if path == "/api/terminal_pty/status" {
        return match service.snapshot().await {
            Ok(value) => json_response(StatusCode::OK, value, head, "").unwrap_or_else(|error| {
                crate::http_response::error_response(error.to_string(), head)
            }),
            Err(error) => crate::http_response::error_response(error.to_string(), head),
        };
    }
    if let Some(error) = handshake::handshake_error(request.headers()) {
        return text(StatusCode::BAD_REQUEST, &error, head);
    }
    let Some(key) = request.headers().get(header::SEC_WEBSOCKET_KEY) else {
        return text(StatusCode::BAD_REQUEST, "Handshake error", head);
    };
    let accept = match header::HeaderValue::try_from(derive_accept_key(key.as_bytes())) {
        Ok(accept) => accept,
        Err(_) => return text(StatusCode::BAD_REQUEST, "Handshake error", head),
    };
    let mut session = query(&request, "session");
    session = session.trim().to_owned();
    if session.is_empty() {
        session.clone_from(&service.config.web_session);
    }
    let rows = query(&request, "rows");
    let cols = query(&request, "cols");
    let spec = Spec {
        mode: if path == "/ws/terminal_pty" {
            Mode::Pty
        } else {
            Mode::Tmux
        },
        session,
        reset: matches!(query(&request, "reset").as_str(), "1" | "true" | "yes"),
        rows: Value::text(if rows.is_empty() { "28" } else { &rows }),
        cols: Value::text(if cols.is_empty() { "100" } else { &cols }),
    };
    if let Err(error) = service.launch(hyper::upgrade::on(&mut request), spec) {
        return crate::http_response::error_response(error.to_string(), head);
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
