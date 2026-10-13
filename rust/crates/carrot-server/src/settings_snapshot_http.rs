use crate::{
    http::{Application, Body, RequestBody},
    http_response::{error_response, json_response, text},
    Value,
};
use hyper::{header, Method, Request, Response, StatusCode};
use std::sync::Arc;

pub(crate) async fn handle(request: Request<RequestBody>, app: Arc<Application>) -> Response<Body> {
    let head = request.method() == Method::HEAD;
    if request.method() != Method::GET && !head {
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
    if !app.config.settings.exists() {
        let value = Value::object([
            ("ok", Value::Bool(false)),
            (
                "error",
                Value::text(&format!(
                    "settings file not found: {}",
                    app.config.settings.display()
                )),
            ),
        ]);
        return json_response(StatusCode::NOT_FOUND, value, head, "")
            .unwrap_or_else(|error| error_response(error.to_string(), head));
    }
    if let Err(error) = app.popular_values.schedule_now(Arc::clone(&app)) {
        return error_response(error.to_string(), head);
    }
    let coding = request
        .headers()
        .get(header::ACCEPT_ENCODING)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_owned();
    match tokio::task::spawn_blocking(move || crate::settings_snapshot::payload(&app)).await {
        Ok(Ok(value)) => match json_response(StatusCode::OK, value, head, &coding) {
            Ok(mut response) => {
                response.headers_mut().insert(
                    header::CACHE_CONTROL,
                    header::HeaderValue::from_static("no-store"),
                );
                response
            }
            Err(error) => error_response(error.to_string(), head),
        },
        Ok(Err(error)) => error_response(error.to_string(), head),
        Err(error) => error_response(error.to_string(), head),
    }
}
