use super::{context::Reply, service::Service};
use crate::{
    http::{Body, RequestBody},
    http_response::{json_response, text},
    Error, Value,
};
use hyper::{header, Method, Request, Response, StatusCode};
use num_traits::ToPrimitive;
use std::sync::Arc;

pub fn matches(path: &str) -> bool {
    matches!(
        path,
        "/api/tools"
            | "/api/tools/start"
            | "/api/tools/job"
            | "/api/tools/jobs"
            | "/api/tools/jobs/notice"
            | "/api/tools/device_info"
    )
}
fn internal(head: bool) -> Response<Body> {
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
fn response(reply: Reply, head: bool) -> Response<Body> {
    json_response(
        StatusCode::from_u16(reply.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        reply.value,
        head,
        "",
    )
    .unwrap_or_else(|_| internal(head))
}
fn query(request: &Request<RequestBody>, name: &str) -> String {
    url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes())
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
        .unwrap_or_default()
}
pub async fn handle(request: Request<RequestBody>, service: Arc<Service>) -> Response<Body> {
    let head = request.method() == Method::HEAD;
    let method = request.method().clone();
    let path = percent_encoding::percent_decode_str(request.uri().path())
        .decode_utf8_lossy()
        .into_owned();
    let allow = match path.as_str() {
        "/api/tools/jobs" => "DELETE,GET,HEAD",
        "/api/tools/job" | "/api/tools/device_info" => "GET,HEAD",
        _ => "POST",
    };
    let allowed = match allow {
        "DELETE,GET,HEAD" => matches!(method, Method::DELETE | Method::GET | Method::HEAD),
        "GET,HEAD" => matches!(method, Method::GET | Method::HEAD),
        _ => method == Method::POST,
    };
    if !allowed {
        let mut response = text(
            StatusCode::METHOD_NOT_ALLOWED,
            "405: Method Not Allowed",
            head,
        );
        response
            .headers_mut()
            .insert(header::ALLOW, header::HeaderValue::from_static(allow));
        return response;
    }
    let reply = match path.as_str() {
        "/api/tools/job" => {
            let id = query(&request, "id").trim().to_owned();
            if id.is_empty() {
                Ok(Reply::error(400, "missing job id"))
            } else {
                service.jobs.get(&id).map(|job| match job {
                    Some(job) => Reply::ok(job),
                    None => Reply::error(404, "job not found"),
                })
            }
        }
        "/api/tools/jobs" => {
            if method == Method::DELETE {
                service.jobs.clear().and_then(|removed| {
                    Ok(Reply::ok(Value::object([
                        ("ok", Value::Bool(true)),
                        ("removed", Value::integer(removed)),
                        ("jobs", service.jobs.snapshots(20)?),
                    ])))
                })
            } else {
                let limit = query(&request, "limit");
                let number = Value::text(limit.trim())
                    .int()
                    .unwrap_or_else(|_| 20.into())
                    .max(1.into())
                    .min(20.into())
                    .to_usize()
                    .unwrap_or(20);
                service.jobs.snapshots(number).map(|jobs| {
                    Reply::ok(Value::object([("ok", Value::Bool(true)), ("jobs", jobs)]))
                })
            }
        }
        "/api/tools/device_info" => {
            let config = Arc::clone(&service.config);
            tokio::task::spawn_blocking(move || super::info::snapshot(&config))
                .await
                .map(|info| Reply::ok(Value::object([("ok", Value::Bool(true)), ("info", info)])))
                .map_err(|error| Error::Source(error.to_string()))
        }
        _ => {
            let body = match crate::http_request::read_json(request).await {
                Ok(body) => body,
                Err(_) => return response(Reply::error(400, "invalid json"), head),
            };
            if !matches!(body, Value::Object(_)) {
                return internal(head);
            }
            match path.as_str() {
                "/api/tools/start" => service.start(body),
                "/api/tools/jobs/notice" => notice(&service, &body),
                "/api/tools" => service.sync(body).await,
                _ => Ok(Reply::error(404, "404: Not Found")),
            }
        }
    };
    match reply {
        Ok(reply) => response(reply, head),
        Err(_) => internal(head),
    }
}
fn notice(service: &Service, body: &Value) -> Result<Reply, Error> {
    let message = crate::param_changes::text::stripped(body.get("message"), true)?;
    if !message.truth() {
        return Ok(Reply::error(400, "missing message"));
    }
    let action = crate::param_changes::text::stripped(body.get("action"), true)?;
    let id = service.jobs.create(
        if action.truth() {
            action
        } else {
            Value::text("notice")
        },
        Value::object([("notice", Value::Bool(true))]),
        Some(message),
    )?;
    Ok(Reply::ok(Value::object([
        ("ok", Value::Bool(true)),
        ("job", service.jobs.get(&id)?.unwrap_or(Value::Null)),
        ("jobs", service.jobs.snapshots(20)?),
    ])))
}
