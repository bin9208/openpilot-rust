use super::{upload_http_parse as parse, upload_http_service::Uploads, Failure};
use crate::{
    http::{Body, RequestBody},
    http_response::{json_response, text},
    Value,
};
use hyper::{header, Method, Request, Response, StatusCode};
use std::sync::Arc;

enum Route {
    Summary,
    Start,
    Job,
    Cancel,
}
fn decoded_is(value: &str, expected: &str) -> bool {
    percent_encoding::percent_decode_str(value)
        .decode_utf8()
        .is_ok_and(|value| value == expected)
}
fn route(path: &str) -> Option<Route> {
    let mut parts = path.strip_prefix('/')?.split('/');
    for expected in ["api", "dashcam", "upload"] {
        if !decoded_is(parts.next()?, expected) {
            return None;
        }
    }
    let operation = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    [
        ("summary", Route::Summary),
        ("start", Route::Start),
        ("job", Route::Job),
        ("cancel", Route::Cancel),
    ]
    .into_iter()
    .find_map(|(name, route)| decoded_is(operation, name).then_some(route))
}
pub fn matches(path: &str) -> bool {
    route(path).is_some()
}
fn failed(head: bool) -> Response<Body> {
    let mut response = text(
        StatusCode::INTERNAL_SERVER_ERROR,
        "500 Internal Server Error\n\nServer got itself in trouble",
        head,
    );
    response.headers_mut().insert(
        header::CONNECTION,
        header::HeaderValue::from_static("close"),
    );
    response
        .extensions_mut()
        .insert(hyper::ext::CloseAfterResponse);
    response
}
fn reply(status: u16, payload: Value, head: bool) -> Response<Body> {
    json_response(
        StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        payload,
        head,
        "",
    )
    .unwrap_or_else(|_| failed(head))
}
fn rejected(status: u16, message: &str, head: bool) -> Response<Body> {
    reply(
        status,
        Value::object([("ok", Value::Bool(false)), ("error", Value::text(message))]),
        head,
    )
}
pub async fn handle(request: Request<RequestBody>, uploads: Arc<Uploads>) -> Response<Body> {
    let head = request.method() == Method::HEAD;
    let Some(route) = route(request.uri().path()) else {
        return text(StatusCode::NOT_FOUND, "404: Not Found", head);
    };
    let (allowed, methods) = match route {
        Route::Job => (
            matches!(request.method(), &Method::GET | &Method::HEAD),
            "GET,HEAD",
        ),
        Route::Summary | Route::Start | Route::Cancel => (request.method() == Method::POST, "POST"),
    };
    if !allowed {
        let mut response = text(
            StatusCode::METHOD_NOT_ALLOWED,
            "405: Method Not Allowed",
            head,
        );
        response
            .headers_mut()
            .insert(header::ALLOW, header::HeaderValue::from_static(methods));
        return response;
    }
    let result = match route {
        Route::Job => {
            let id = url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes())
                .find(|(key, _)| key == "id")
                .map(|(_, value)| value.into_owned())
                .unwrap_or_default();
            let id = crate::param_changes::text::stripped(&Value::text(&id), false)
                .and_then(|value| value.string().map_err(crate::Error::from));
            let id = match id {
                Ok(id) if !id.is_empty() => id,
                Ok(_) => return rejected(400, "missing job id", head),
                Err(_) => return failed(head),
            };
            uploads.snapshot(id).await
        }
        Route::Summary | Route::Start | Route::Cancel => {
            let body = match crate::http_request::read_json_detailed(request).await {
                Ok(body) => body,
                Err(error) => {
                    if let Some(response) = crate::http_response::parser_response(&error, head) {
                        return response;
                    }
                    Value::Object(Vec::new())
                }
            };
            match route {
                Route::Summary => match parse::segments(&body) {
                    Ok(segments) => {
                        match tokio::task::spawn_blocking(move || uploads.summary(&segments)).await
                        {
                            Ok(result) => result,
                            Err(_) => return rejected(500, "dashcam upload summary failed", head),
                        }
                    }
                    Err(error) => Err(error),
                },
                Route::Start => match parse::segments(&body) {
                    Ok(segments) => uploads.start(segments).await,
                    Err(error) => Err(error),
                },
                Route::Cancel => {
                    let id = match parse::cancel_id(&body) {
                        Ok(id) if !id.is_empty() => id,
                        Ok(_) => return rejected(400, "missing job id", head),
                        Err(_) => return failed(head),
                    };
                    uploads.cancel(id).await
                }
                Route::Job => return failed(head),
            }
        }
    };
    match result {
        Ok(payload) => {
            let status = if payload.get("ok").truth() {
                200
            } else if matches!(route, Route::Start) {
                409
            } else {
                404
            };
            reply(status, payload, head)
        }
        Err(Failure::Http { status, message }) => rejected(status, &message, head),
        Err(error) if matches!(route, Route::Summary | Route::Start) => {
            rejected(500, &error.to_string(), head)
        }
        Err(_) => failed(head),
    }
}
