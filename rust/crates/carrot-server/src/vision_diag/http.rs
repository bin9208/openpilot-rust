use super::{
    snapshot,
    upload::{self, Upload},
    Config,
};
use crate::{
    http::{Application, Body, RequestBody},
    http_response::{error_response, json_response, text},
    Error, Value,
};
use hyper::{header, Method, Request, Response, StatusCode};
use std::sync::Arc;

pub fn matches(path: &str) -> bool {
    matches!(
        path,
        "/api/vision_diag/server_snapshot" | "/api/vision_diag/upload_discord"
    )
}
fn json(status: StatusCode, value: Value, head: bool) -> Response<Body> {
    json_response(status, value, head, "")
        .unwrap_or_else(|error| error_response(error.to_string(), head))
}
fn reject(status: StatusCode, message: &str) -> Response<Body> {
    json(
        status,
        Value::object([("ok", Value::Bool(false)), ("error", Value::text(message))]),
        false,
    )
}
pub async fn handle(request: Request<RequestBody>, app: Arc<Application>) -> Response<Body> {
    let head = request.method() == Method::HEAD;
    let snapshot_route = request.uri().path() == "/api/vision_diag/server_snapshot";
    if !(if snapshot_route {
        request.method() == Method::GET || head
    } else {
        request.method() == Method::POST
    }) {
        let mut response = text(
            StatusCode::METHOD_NOT_ALLOWED,
            "405: Method Not Allowed",
            head,
        );
        response.headers_mut().insert(
            header::ALLOW,
            header::HeaderValue::from_static(if snapshot_route { "GET,HEAD" } else { "POST" }),
        );
        return response;
    }
    let config = match &app.vision_diag {
        Some(config) => Ok(config.clone()),
        None => Config::original(&app),
    };
    let config = match config {
        Ok(config) => config,
        Err(error) => return error_response(error.to_string(), head),
    };
    let history = app
        .stream_proxy
        .as_ref()
        .map(|service| service.history.clone())
        .unwrap_or_default();
    if snapshot_route {
        return match snapshot::collect(config, history).await {
            Ok(snapshot) => json(
                StatusCode::OK,
                Value::object([("ok", Value::Bool(true)), ("snapshot", snapshot)]),
                head,
            ),
            Err(error) => error_response(error.to_string(), head),
        };
    }
    let body = match crate::http_request::read_json_detailed(request).await {
        Ok(body) => body,
        Err(Error::Source(message)) if message == "Request Entity Too Large" => {
            return reject(StatusCode::PAYLOAD_TOO_LARGE, "diagnostic upload too large")
        }
        Err(error) => {
            return crate::http_response::parser_response(&error, false).unwrap_or_else(|| {
                reject(
                    StatusCode::BAD_REQUEST,
                    &format!("invalid diagnostic upload json: {error}"),
                )
            })
        }
    };
    let input = match Upload::parse(&body) {
        Ok(input) => input,
        Err(_) => {
            return text(
                StatusCode::INTERNAL_SERVER_ERROR,
                "500 Internal Server Error\n\nServer got itself in trouble",
                false,
            )
        }
    };
    if input
        .bundle
        .trim_matches(crate::tools::text::whitespace)
        .is_empty()
    {
        return reject(StatusCode::BAD_REQUEST, "missing diagnostic bundle");
    }
    match upload::send(config, history, input).await {
        Ok(result) => json(
            if result.get("ok").truth() || result.get("skipped").truth() {
                StatusCode::OK
            } else {
                StatusCode::BAD_GATEWAY
            },
            Value::object([
                ("ok", Value::Bool(result.get("ok").truth())),
                ("discord", result),
            ]),
            false,
        ),
        Err(error) => error_response(error.to_string(), false),
    }
}
