use super::Config;
use crate::{
    http::{Body, RequestBody},
    http_response::{json_response, text},
    Error, Value,
};
use hyper::{header, Method, Request, Response, StatusCode};

pub async fn handle(request: Request<RequestBody>, config: Option<Config>) -> Response<Body> {
    let head = request.method() == Method::HEAD;
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
    let result = tokio::task::spawn_blocking(move || {
        let config = match config {
            Some(config) => config,
            None => Config::original()?,
        };
        let status = super::status::get(&config)?;
        let mut fields = vec![("ok".chars().map(u32::from).collect(), Value::Bool(true))];
        if let Value::Object(status) = status {
            for (key, value) in status {
                crate::json_fields::insert(&mut fields, key, value);
            }
        }
        Ok::<_, Error>(Value::Object(fields))
    })
    .await;
    match result {
        Ok(Ok(value)) => json_response(StatusCode::OK, value, head, "")
            .unwrap_or_else(|error| crate::http_response::error_response(error.to_string(), head)),
        Ok(Err(error)) => crate::http_response::error_response(error.to_string(), head),
        Err(error) => crate::http_response::error_response(error.to_string(), head),
    }
}
