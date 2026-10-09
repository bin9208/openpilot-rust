use super::{cache::Service, pages, Failure};
use crate::{
    http::{Body, RequestBody},
    http_response::{json_response, text},
    Error, Value,
};
use hyper::{
    header::{self, HeaderValue},
    Method, Request, Response, StatusCode,
};
use std::sync::Arc;

enum Route<'a> {
    Routes,
    Segments(&'a str),
    Recent,
    ReadState,
}
fn decoded_is(value: &str, literal: &str) -> bool {
    percent_encoding::percent_decode_str(value)
        .decode_utf8()
        .is_ok_and(|value| value == literal)
}
fn route(path: &str) -> Option<Route<'_>> {
    let mut parts = path.strip_prefix('/')?.split('/');
    if !decoded_is(parts.next()?, "api") || !decoded_is(parts.next()?, "dashcam") {
        return None;
    }
    let kind = parts.next()?;
    if decoded_is(kind, "segments") {
        let raw = parts.next().filter(|value| !value.is_empty())?;
        return parts.next().is_none().then_some(Route::Segments(raw));
    }
    if parts.next().is_some() {
        return None;
    }
    if decoded_is(kind, "routes") {
        Some(Route::Routes)
    } else if decoded_is(kind, "recent") {
        Some(Route::Recent)
    } else if decoded_is(kind, "read-state") {
        Some(Route::ReadState)
    } else {
        None
    }
}
pub fn matches(path: &str) -> bool {
    route(path).is_some()
}
fn query(request: &Request<RequestBody>, name: &str) -> Option<String> {
    url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes())
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
}
fn reply(status: u16, value: Value, head: bool) -> Response<Body> {
    json_response(
        StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        value,
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
fn failed(head: bool) -> Response<Body> {
    let mut response = text(
        StatusCode::INTERNAL_SERVER_ERROR,
        "500 Internal Server Error\n\nServer got itself in trouble",
        head,
    );
    response
        .headers_mut()
        .insert(header::CONNECTION, HeaderValue::from_static("close"));
    response
        .extensions_mut()
        .insert(hyper::ext::CloseAfterResponse);
    response
}
fn state_payload(value: Value) -> Value {
    let mut result = Value::object([("ok", Value::Bool(true))]);
    if let (Value::Object(result), Value::Object(fields)) = (&mut result, value) {
        result.extend(fields);
    }
    result
}
async fn write_state(
    request: Request<RequestBody>,
    service: Arc<Service>,
    head: bool,
) -> Response<Body> {
    let body = match crate::http_request::read_json_detailed(request).await {
        Ok(value) => value,
        Err(error) => {
            if let Some(response) = crate::http_response::parser_response(&error, head) {
                return response;
            }
            Value::Object(Vec::new())
        }
    };
    if !matches!(body, Value::Object(_)) {
        return rejected(400, "bad request", head);
    }
    let result = tokio::task::spawn_blocking(move || {
        service
            .read_state
            .write(body.get("recentSegment"), service.wall())
    })
    .await;
    match result {
        Ok(Ok(value)) => reply(200, state_payload(value), head),
        Ok(Err(Failure::InvalidRecent)) => rejected(400, "invalid recent segment", head),
        Ok(Err(Failure::Runtime(Error::Json(error))))
            if matches!(
                error.kind,
                "ValueError" | "UnicodeEncodeError" | "UnicodeDecodeError"
            ) =>
        {
            rejected(400, &error.to_string(), head)
        }
        Ok(Err(Failure::Http { .. } | Failure::Runtime(_))) | Err(_) => failed(head),
    }
}
pub async fn handle(request: Request<RequestBody>, service: Arc<Service>) -> Response<Body> {
    let head = request.method() == Method::HEAD;
    let Some(route) = route(request.uri().path()) else {
        return text(StatusCode::NOT_FOUND, "404: Not Found", head);
    };
    let state = matches!(route, Route::ReadState);
    if state && request.method() == Method::POST {
        return write_state(request, service, head).await;
    }
    if !matches!(request.method(), &Method::GET | &Method::HEAD) {
        let mut result = text(
            StatusCode::METHOD_NOT_ALLOWED,
            "405: Method Not Allowed",
            head,
        );
        result.headers_mut().insert(
            header::ALLOW,
            HeaderValue::from_static(if state { "GET,HEAD,POST" } else { "GET,HEAD" }),
        );
        return result;
    }
    let offset = pages::bounded(query(&request, "offset"), "offset", 0, 1_000_000);
    let sort = query(&request, "sort")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "asc".into());
    let descending = sort
        .trim_matches(|character: char| {
            character.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&character)
        })
        .to_lowercase()
        == "desc";
    let (result, bare_errors) = match route {
        Route::Routes => {
            let limit = pages::bounded(query(&request, "limit"), "limit", 40, 200);
            let segment_limit =
                pages::bounded(query(&request, "segment_limit"), "segment_limit", 10, 2000);
            (
                tokio::task::spawn_blocking(move || {
                    pages::routes_payload(&service, offset, limit, segment_limit, descending)
                })
                .await,
                false,
            )
        }
        Route::Segments(raw) => {
            let route = percent_encoding::percent_decode_str(raw)
                .decode_utf8_lossy()
                .into_owned();
            let limit = pages::bounded(query(&request, "limit"), "limit", 10, 2000);
            let result = tokio::task::spawn_blocking(move || {
                pages::segments_payload(&service, &route, offset, limit, descending)
            })
            .await;
            return match result {
                Ok(Ok(Some(value))) => reply(200, value, head),
                Ok(Ok(None)) => rejected(404, "route not found", head),
                Ok(Err(error)) => rejected(500, &error.to_string(), head),
                Err(_) => rejected(500, "dashcam operation failed", head),
            };
        }
        Route::Recent => {
            let limit = query(&request, "limit")
                .filter(|value| !value.is_empty())
                .and_then(|value| Value::text(&value).int().ok())
                .and_then(|value| num_traits::ToPrimitive::to_usize(&value));
            let Some(limit @ (2 | 5 | 10)) = limit else {
                return rejected(400, "limit must be one of 2, 5, 10", head);
            };
            (
                tokio::task::spawn_blocking(move || {
                    service.recent(limit).map(|segments| {
                        Value::object([
                            ("ok", Value::Bool(true)),
                            ("requested", Value::integer(limit)),
                            ("count", Value::integer(segments.len())),
                            ("segments", Value::Array(segments)),
                        ])
                    })
                })
                .await,
                false,
            )
        }
        Route::ReadState => (
            tokio::task::spawn_blocking(move || service.read_state.read().map(state_payload)).await,
            true,
        ),
    };
    match result {
        Ok(Ok(value)) => reply(200, value, head),
        Ok(Err(error)) if !bare_errors => rejected(500, &error.to_string(), head),
        Ok(Err(_)) | Err(_) => failed(head),
    }
}
