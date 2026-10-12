use super::{cache, Service};
use crate::{
    http::{Application, Body, RequestBody},
    http_response::{json_response, text},
    Error, Value,
};
use hyper::{header, Method, Request, Response, StatusCode};
use std::sync::Arc;

pub fn matches(path: &str) -> bool {
    matches!(
        path,
        "/api/setting_popular_values"
            | "/api/setting_popular_values/detail"
            | "/api/setting_popular_values/refresh"
    )
}

fn failure(head: bool) -> Response<Body> {
    text(
        StatusCode::INTERNAL_SERVER_ERROR,
        "500 Internal Server Error\n\nServer got itself in trouble",
        head,
    )
}

pub async fn handle(
    request: Request<RequestBody>,
    app: Arc<Application>,
    service: Arc<Service>,
) -> Response<Body> {
    let head = request.method() == Method::HEAD;
    let path = percent_encoding::percent_decode_str(request.uri().path()).decode_utf8_lossy();
    let refresh = path.ends_with("/refresh");
    if (refresh && request.method() != Method::POST)
        || (!refresh && !matches!(request.method(), &Method::GET | &Method::HEAD))
    {
        let mut result = text(
            StatusCode::METHOD_NOT_ALLOWED,
            "405: Method Not Allowed",
            head,
        );
        result.headers_mut().insert(
            header::ALLOW,
            header::HeaderValue::from_static(if refresh { "POST" } else { "GET,HEAD" }),
        );
        return result;
    }
    let result: Result<Value, Error> = if refresh {
        service.refresh(&app, true).await
    } else if path.ends_with("/detail") {
        let name = url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes())
            .find(|(key, _)| key == "name")
            .map(|(_, value)| value.into_owned())
            .unwrap_or_default();
        service.read_application(&app).map(|value| {
            Value::object([
                ("ok", Value::Bool(true)),
                (
                    "car_key_type",
                    if value.has("car_key_type") {
                        value.get("car_key_type").clone()
                    } else {
                        Value::text("CarSelected3")
                    },
                ),
                (
                    "car_key",
                    if value.has("car_key") {
                        value.get("car_key").clone()
                    } else {
                        Value::text("")
                    },
                ),
                ("param_name", Value::text(&name)),
                ("detail", cache::detail(&value, &name)),
            ])
        })
    } else {
        service
            .schedule_now(Arc::clone(&app))
            .and_then(|_| service.read_application(&app))
    };
    match result {
        Ok(value) => {
            json_response(StatusCode::OK, value, head, "").unwrap_or_else(|_| failure(head))
        }
        Err(_) => failure(head),
    }
}
