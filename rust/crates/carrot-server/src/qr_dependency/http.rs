use crate::{
    http::{Application, Body, RequestBody},
    http_response::{json_response, text},
    Value,
};
use hyper::{header, Method, Request, Response, StatusCode};
use std::sync::Arc;

pub(crate) fn matches(path: &str) -> bool {
    matches!(
        path,
        "/api/params_qr_dependency" | "/api/params_qr_dependency/ensure"
    )
}

pub(crate) async fn handle(
    request: Request<RequestBody>,
    app: Arc<Application>,
    path: &str,
) -> Response<Body> {
    let ensure = path.ends_with("/ensure");
    let head = request.method() == Method::HEAD;
    if (ensure && request.method() != Method::POST)
        || (!ensure && !matches!(request.method(), &Method::GET | &Method::HEAD))
    {
        let mut response = text(
            StatusCode::METHOD_NOT_ALLOWED,
            "405: Method Not Allowed",
            head,
        );
        response.headers_mut().insert(
            header::ALLOW,
            header::HeaderValue::from_static(if ensure { "POST" } else { "GET,HEAD" }),
        );
        return response;
    }
    let value = tokio::task::spawn_blocking(move || {
        if ensure {
            app.qr_dependency.ensure()
        } else {
            app.qr_dependency.status()
        }
    })
    .await;
    let (status, value) = match value {
        Ok(value) => (
            if value.get("ok").truth() {
                StatusCode::OK
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            },
            value,
        ),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Value::object([
                ("ok", Value::Bool(false)),
                ("error", Value::text(&error.to_string())),
            ]),
        ),
    };
    json_response(status, value, head, "").unwrap_or_else(|_| {
        text(
            StatusCode::INTERNAL_SERVER_ERROR,
            "500 Internal Server Error\n\nServer got itself in trouble",
            head,
        )
    })
}
