use super::{history::Outcome, summary, Service};
use crate::{
    http::{Body, RequestBody},
    http_response::{error_response, json_response, response, text},
    Error, Value,
};
use http_body_util::BodyExt;
use hyper::{header, Method, Request, Response, StatusCode};
use std::{net::SocketAddr, sync::Arc, time::Instant};

struct Reply {
    status: StatusCode,
    content_type: String,
    bytes: Vec<u8>,
}
#[derive(Debug, thiserror::Error)]
enum ForwardError {
    #[error(transparent)]
    Network(#[from] reqwest::Error),
    #[error(transparent)]
    Content(#[from] crate::DecodeFailure),
}
impl ForwardError {
    fn timed_out(&self) -> bool {
        match self {
            Self::Network(error) => error.is_timeout(),
            Self::Content(_) => false,
        }
    }
}
async fn forward(
    service: &Service,
    bytes: Vec<u8>,
    content_type: String,
) -> Result<Reply, ForwardError> {
    let mut reply = service
        .client
        .post(&service.target)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::ACCEPT_ENCODING, "gzip, deflate, br")
        .body(bytes)
        .send()
        .await?;
    let status = reply.status();
    let content_type = reply
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_owned();
    let headers = reply.headers().clone();
    let mut bytes = Vec::new();
    while let Some(chunk) = reply.chunk().await? {
        bytes.extend_from_slice(&chunk);
    }
    Ok(Reply {
        status,
        content_type,
        bytes: decode(&headers, &bytes)?,
    })
}
fn decode(headers: &hyper::HeaderMap, bytes: &[u8]) -> Result<Vec<u8>, crate::DecodeFailure> {
    let mut decoder = crate::request_body::decoder::Decoder::new(headers);
    let output = decoder.feed(bytes)?;
    decoder.finish()?;
    Ok(output)
}
fn json(status: StatusCode, value: Value) -> Response<Body> {
    json_response(status, value, false, "")
        .unwrap_or_else(|error| error_response(error.to_string(), false))
}
pub async fn handle(request: Request<RequestBody>, service: Arc<Service>) -> Response<Body> {
    if request.method() != Method::POST {
        let mut response = text(
            StatusCode::METHOD_NOT_ALLOWED,
            "405: Method Not Allowed",
            request.method() == Method::HEAD,
        );
        response
            .headers_mut()
            .insert(header::ALLOW, header::HeaderValue::from_static("POST"));
        return response;
    }
    let content_type = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("application/json")
        .to_owned();
    let client = request
        .extensions()
        .get::<SocketAddr>()
        .map_or_else(String::new, |peer| peer.ip().to_string());
    let mut body = request.into_body();
    let mut bytes = Vec::new();
    while let Some(frame) = body.frame().await {
        let frame = match frame {
            Ok(frame) => frame,
            Err(error) => {
                return crate::http_response::parser_response(&Error::Request(error.clone()), false)
                    .unwrap_or_else(|| error_response(error.to_string(), false))
            }
        };
        if let Ok(data) = frame.into_data() {
            let size = bytes.len().saturating_add(data.len());
            if size >= crate::config::BODY_LIMIT {
                return text(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    &format!(
                        "Maximum request body size {} exceeded, actual body size {size}",
                        crate::config::BODY_LIMIT
                    ),
                    false,
                );
            }
            bytes.extend_from_slice(&data);
        }
    }
    let started = Instant::now();
    let mut base = Value::object([
        ("client", Value::text(&client)),
        ("content_type", Value::text(&content_type)),
        ("request_bytes", Value::integer(bytes.len())),
    ]);
    if let (Value::Object(base), Value::Object(summary)) = (&mut base, summary::parse(&bytes)) {
        base.extend(summary);
    }
    let (outcome, reply) = if service.cluster_active() {
        (
            Outcome::Error {
                status: 409,
                message: "cluster HUD active".into(),
            },
            json(
                StatusCode::CONFLICT,
                Value::object([
                    ("ok", Value::Bool(false)),
                    (
                        "error",
                        Value::text("Carrot Vision is unavailable while Cluster HUD is active"),
                    ),
                    ("code", Value::text("cluster_hud_active")),
                ]),
            ),
        )
    } else {
        match forward(&service, bytes, content_type).await {
            Ok(reply) => (
                Outcome::Response {
                    status: reply.status.as_u16(),
                    bytes: reply.bytes.len(),
                },
                response(reply.status, reply.bytes, &reply.content_type, false),
            ),
            Err(error) => {
                let (status, message) = if error.timed_out() {
                    (StatusCode::GATEWAY_TIMEOUT, "webrtcd timeout".into())
                } else {
                    (StatusCode::BAD_GATEWAY, error.to_string())
                };
                let reply = json(
                    status,
                    Value::object([("ok", Value::Bool(false)), ("error", Value::text(&message))]),
                );
                (
                    Outcome::Error {
                        status: status.as_u16(),
                        message,
                    },
                    reply,
                )
            }
        }
    };
    if let Err(error) = service.history.record(base, outcome, started) {
        eprintln!("Stream history: {error}");
    }
    reply
}
