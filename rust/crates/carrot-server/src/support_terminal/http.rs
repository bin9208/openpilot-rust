use super::{
    assets,
    context::{Context, Event},
    session::Mode,
    Service, StartOptions,
};
use crate::{
    http::{Body, RequestBody},
    http_response::{json_response, text},
    web_sound_http::handshake,
    Error, Value,
};
use bytes::Bytes;
use http_body_util::Full;
use hyper::{header, Method, Request, Response, StatusCode};
use std::{rc::Rc, sync::Arc};
use tokio_tungstenite::tungstenite::handshake::derive_accept_key;

enum Route {
    Start,
    Stop,
    Status,
    Decision(String, bool),
    Owner,
    Guest(String),
    Page(String),
    Asset(String),
}
fn segment(path: &str, prefix: &str) -> Option<String> {
    path.strip_prefix(prefix)
        .filter(|tail| !tail.is_empty() && !tail.contains('/'))
        .map(str::to_owned)
}
fn route(path: &str, local: bool) -> Option<Route> {
    if local && path == "/" {
        return Some(Route::Page(String::new()));
    }
    if !local {
        match path {
            "/api/support_terminal/start" => return Some(Route::Start),
            "/api/support_terminal/stop" => return Some(Route::Stop),
            "/api/support_terminal/status" => return Some(Route::Status),
            "/ws/support_terminal/owner" => return Some(Route::Owner),
            _ => {}
        }
        if let Some(tail) = path.strip_prefix("/api/support_terminal/commands/") {
            if let Some((id, action)) = tail.split_once('/') {
                if !id.is_empty() && matches!(action, "approve" | "reject") {
                    return Some(Route::Decision(id.into(), action == "approve"));
                }
            }
        }
    }
    if let Some(id) = segment(path, "/ws/support_terminal/") {
        return Some(Route::Guest(id));
    }
    if let Some(id) = segment(path, "/support/terminal/") {
        return Some(Route::Page(id));
    }
    segment(path, "/support-terminal-assets/").map(Route::Asset)
}
pub fn matches(path: &str) -> bool {
    route(path, false).is_some()
}
fn json(value: Value, status: StatusCode, head: bool) -> Response<Body> {
    json_response(status, value, head, "")
        .unwrap_or_else(|error| crate::http_response::error_response(error.to_string(), head))
}
fn method(route: &Route, request: &Request<RequestBody>) -> Option<Response<Body>> {
    let post = matches!(route, Route::Start | Route::Stop | Route::Decision(_, _));
    if if post {
        request.method() != Method::POST
    } else {
        !matches!(request.method(), &Method::GET | &Method::HEAD)
    } {
        let mut response = text(
            StatusCode::METHOD_NOT_ALLOWED,
            "405: Method Not Allowed",
            request.method() == Method::HEAD,
        );
        response.headers_mut().insert(
            header::ALLOW,
            header::HeaderValue::from_static(if post { "POST" } else { "GET,HEAD" }),
        );
        Some(response)
    } else {
        None
    }
}
enum Upgrade {
    Accepted(Response<Body>),
    Rejected(Response<Body>),
}
fn upgrade(request: &mut Request<RequestBody>) -> Upgrade {
    let head = request.method() == Method::HEAD;
    if let Some(error) = handshake::handshake_error(request.headers()) {
        return Upgrade::Rejected(text(StatusCode::BAD_REQUEST, &error, head));
    }
    let Some(key) = request.headers().get(header::SEC_WEBSOCKET_KEY) else {
        return Upgrade::Rejected(text(StatusCode::BAD_REQUEST, "Handshake error: None", head));
    };
    let accept = match header::HeaderValue::try_from(derive_accept_key(key.as_bytes())) {
        Ok(accept) => accept,
        Err(error) => {
            return Upgrade::Rejected(crate::http_response::error_response(
                error.to_string(),
                head,
            ))
        }
    };
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
    Upgrade::Accepted(response)
}
pub async fn handle(mut request: Request<RequestBody>, service: Arc<Service>) -> Response<Body> {
    let head = request.method() == Method::HEAD;
    let path = percent_encoding::percent_decode_str(request.uri().path()).decode_utf8_lossy();
    let Some(route) = route(&path, false) else {
        return text(StatusCode::NOT_FOUND, "404: Not Found", head);
    };
    if let Some(response) = method(&route, &request) {
        return response;
    }
    let result = match route {
        Route::Start => {
            let value = crate::http::read_json(request).await.unwrap_or(Value::Null);
            match StartOptions::parse(&value, &service.config) {
                Ok(options) => service.start_session(options).await,
                Err(error) => Err(error),
            }
        }
        Route::Stop => service.stop_session().await,
        Route::Status => service.snapshot().await,
        Route::Decision(id, approve) => service.decision(id, approve).await,
        Route::Owner | Route::Guest(_) => {
            let mode = match route {
                Route::Owner => Mode::Owner,
                Route::Guest(id) => Mode::Guest(id),
                _ => return text(StatusCode::NOT_FOUND, "404: Not Found", head),
            };
            let response = match upgrade(&mut request) {
                Upgrade::Accepted(response) => response,
                Upgrade::Rejected(response) => return response,
            };
            return match service.launch(hyper::upgrade::on(&mut request), mode) {
                Ok(()) => response,
                Err(error) => crate::http_response::error_response(error.to_string(), head),
            };
        }
        Route::Page(id) => {
            return asset_result(assets::page(&request, &service.config, &id).await, head)
        }
        Route::Asset(name) => {
            return asset_result(assets::file(&request, &service.config, &name).await, head)
        }
    };
    match result {
        Ok(value) => {
            let status = if value.get("state").text_eq("error") {
                StatusCode::INTERNAL_SERVER_ERROR
            } else if !value.get("ok").truth() {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::OK
            };
            json(value, status, head)
        }
        Err(error) => crate::http_response::error_response(error.to_string(), head),
    }
}
fn asset_result(result: Result<Response<Body>, Error>, head: bool) -> Response<Body> {
    result.unwrap_or_else(|error| {
        eprintln!("Support asset: {error}");
        text(
            StatusCode::INTERNAL_SERVER_ERROR,
            "500 Internal Server Error\n\nServer got itself in trouble",
            head,
        )
    })
}
pub(super) async fn guest(
    mut request: Request<RequestBody>,
    context: Rc<Context>,
) -> Response<Body> {
    let head = request.method() == Method::HEAD;
    let path = percent_encoding::percent_decode_str(request.uri().path()).decode_utf8_lossy();
    let Some(route) = route(&path, true) else {
        return text(StatusCode::NOT_FOUND, "404: Not Found", head);
    };
    if let Some(response) = method(&route, &request) {
        return response;
    }
    match route {
        Route::Page(id) => asset_result(assets::page(&request, &context.config, &id).await, head),
        Route::Asset(name) => {
            asset_result(assets::file(&request, &context.config, &name).await, head)
        }
        Route::Guest(id) => {
            let response = match upgrade(&mut request) {
                Upgrade::Accepted(response) => response,
                Upgrade::Rejected(response) => return response,
            };
            let admission = match context.count.admit() {
                Ok(admission) => admission,
                Err(error) => return crate::http_response::error_response(error.to_string(), head),
            };
            match context.event(Event::Socket(
                hyper::upgrade::on(&mut request),
                Mode::Guest(id),
                admission,
            )) {
                Ok(()) => response,
                Err(error) => crate::http_response::error_response(error.to_string(), head),
            }
        }
        Route::Start | Route::Stop | Route::Status | Route::Decision(_, _) | Route::Owner => {
            text(StatusCode::NOT_FOUND, "404: Not Found", head)
        }
    }
}
