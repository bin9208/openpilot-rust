use crate::{
    http::{read_json, Application, Body},
    http_response::{json_response, text},
    web_settings::{resolve_capabilities, WebSettings},
    Value,
};
use hyper::{body::Incoming, header, Method, Request, Response, StatusCode};
use std::sync::Arc;

pub(crate) async fn handle(request: Request<Incoming>, app: Arc<Application>) -> Response<Body> {
    let head = request.method() == Method::HEAD;
    let post = request.method() == Method::POST;
    if !matches!(
        request.method(),
        &Method::GET | &Method::HEAD | &Method::POST
    ) {
        let mut response = text(
            StatusCode::METHOD_NOT_ALLOWED,
            "405: Method Not Allowed",
            head,
        );
        response.headers_mut().insert(
            header::ALLOW,
            header::HeaderValue::from_static("GET,HEAD,POST"),
        );
        return response;
    }
    let body = if post {
        read_json(request)
            .await
            .unwrap_or_else(|_| Value::Object(Vec::new()))
    } else {
        Value::Object(Vec::new())
    };
    if !matches!(body, Value::Object(_)) {
        return json_response(
            StatusCode::BAD_REQUEST,
            Value::object([
                ("ok", Value::Bool(false)),
                ("error", Value::text("bad request")),
            ]),
            head,
            "",
        )
        .unwrap_or_else(|_| {
            text(
                StatusCode::INTERNAL_SERVER_ERROR,
                "500: Internal Server Error",
                head,
            )
        });
    }
    let settings = WebSettings::new(
        &app.config.state.join("web_settings.json"),
        &app.config
            .web
            .join("src/features/drive/core/content_catalog.json"),
    );
    let result = if post {
        settings.update(&body)
    } else {
        settings.read()
    };
    match result {
        Ok(settings) => {
            let capabilities = resolve_capabilities(&settings);
            json_response(
                StatusCode::OK,
                Value::object([
                    ("ok", Value::Bool(true)),
                    ("settings", settings),
                    ("capabilities", capabilities),
                ]),
                head,
                "",
            )
            .unwrap_or_else(|_| {
                text(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "500: Internal Server Error",
                    head,
                )
            })
        }
        Err(_) => text(
            StatusCode::INTERNAL_SERVER_ERROR,
            "500 Internal Server Error\n\nServer got itself in trouble",
            head,
        ),
    }
}
