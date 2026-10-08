use super::{upload_http_parse as parse, upload_sync::SyncUploads, Failure};
use crate::{
    http::{Body, RequestBody},
    http_response::{json_response, text},
    Value,
};
use hyper::{header, Method, Request, Response, StatusCode};
use std::sync::Arc;

pub fn matches(path: &str) -> bool {
    let Some(path) = path.strip_prefix('/') else {
        return false;
    };
    let mut parts = path.split('/');
    ["api", "dashcam", "upload"].into_iter().all(|expected| {
        parts.next().is_some_and(|part| {
            percent_encoding::percent_decode_str(part)
                .decode_utf8()
                .is_ok_and(|part| part == expected)
        })
    }) && parts.next().is_none()
}
pub async fn handle(request: Request<RequestBody>, service: Arc<SyncUploads>) -> Response<Body> {
    let head = request.method() == Method::HEAD;
    if !matches(request.uri().path()) {
        return text(StatusCode::NOT_FOUND, "404: Not Found", head);
    }
    if request.method() != Method::POST {
        let mut response = text(
            StatusCode::METHOD_NOT_ALLOWED,
            "405: Method Not Allowed",
            head,
        );
        response
            .headers_mut()
            .insert(header::ALLOW, header::HeaderValue::from_static("POST"));
        return response;
    }
    let result = async {
        let admission = service.begin()?;
        let body = match crate::http_request::read_json_detailed(request).await {
            Ok(body) => body,
            Err(error) => {
                if let Some(response) = crate::http_response::parser_response(&error, head) {
                    return Ok::<_, Failure>(Err(response));
                }
                Value::Object(Vec::new())
            }
        };
        let segments = parse::segments(&body)?;
        service.upload(admission, segments).await.map(Ok)
    }
    .await;
    let (status, payload) = match result {
        Ok(Ok(payload)) => (200, payload),
        Ok(Err(response)) => return response,
        Err(Failure::Http { status, message }) => (
            status,
            Value::object([("ok", Value::Bool(false)), ("error", Value::text(&message))]),
        ),
        Err(error) => (
            500,
            Value::object([
                ("ok", Value::Bool(false)),
                ("error", Value::text(&error.to_string())),
            ]),
        ),
    };
    json_response(
        StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        payload,
        head,
        "",
    )
    .unwrap_or_else(|_| {
        text(
            StatusCode::INTERNAL_SERVER_ERROR,
            "500 Internal Server Error",
            head,
        )
    })
}
