use super::Service;
use crate::{
    http::Body,
    http_response::{json_response, text},
    Value,
};
use hyper::{header, Method, Request, Response, StatusCode};

pub fn handle<T>(request: &Request<T>, service: &Service) -> Response<Body> {
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
    service
        .snapshot()
        .and_then(|state| {
            json_response(
                StatusCode::OK,
                Value::object([("ok", Value::Bool(true)), ("hb", state)]),
                head,
                "",
            )
        })
        .unwrap_or_else(|_| {
            text(
                StatusCode::INTERNAL_SERVER_ERROR,
                "500 Internal Server Error\n\nServer got itself in trouble",
                head,
            )
        })
}
