use super::{
    session::{Route, Session},
    shared::Shared,
};
use crate::{json::Value, Error};
use bytes::Bytes;
use http_body_util::Full;
use hyper::{body::Incoming, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use std::{convert::Infallible, net::SocketAddr};
use tokio_tungstenite::{
    tungstenite::{
        handshake::derive_accept_key,
        protocol::{Role, WebSocketConfig},
    },
    WebSocketStream,
};

pub type Body = Full<Bytes>;

fn response(status: StatusCode, body: String, content_type: &str) -> Response<Body> {
    let mut result = Response::new(Full::new(Bytes::from(body)));
    *result.status_mut() = status;
    if let Ok(header) = content_type.parse() {
        result.headers_mut().insert("content-type", header);
    }
    result
}
fn text(status: StatusCode, body: &str) -> Response<Body> {
    response(status, body.into(), "text/plain; charset=utf-8")
}

fn decoded(segment: &str) -> String {
    let input = segment.as_bytes();
    let mut bytes = Vec::with_capacity(input.len());
    let mut index = 0;
    while index < input.len() {
        if input[index] == b'%' && index + 2 < input.len() {
            if let Some(byte) = char::from(input[index + 1])
                .to_digit(16)
                .zip(char::from(input[index + 2]).to_digit(16))
                .and_then(|(high, low)| u8::try_from(high * 16 + low).ok())
            {
                bytes.push(byte);
                index += 3;
                continue;
            }
        }
        bytes.push(input[index]);
        index += 1;
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

pub fn route(path: &str) -> Option<Route> {
    let parts: Vec<_> = path.split('/').collect();
    match parts.as_slice() {
        ["", "api", "navi", "ws", "v2", "control", version] if !version.is_empty() => {
            Some(Route::Control(decoded(version)))
        }
        ["", "api", "navi", "ws", "v2", kind, session, name]
            if !session.is_empty() && !name.is_empty() =>
        {
            let kind = match *kind {
                "json" => "json",
                "image" => "image",
                "render" => "render",
                _ => return None,
            };
            Some(Route::Item {
                kind,
                session: decoded(session),
                name: decoded(name),
            })
        }
        _ => None,
    }
}

fn index() -> Value {
    Value::object([
        ("name", Value::text("Carrot Navi Receiver")),
        ("port", Value::integer(7714)),
        ("protocol_version", Value::integer(2)),
        ("health", Value::text("/health")),
        ("latest", Value::text("/api/navi/latest")),
        (
            "ws_control",
            Value::text("/api/navi/ws/v2/control/{version}"),
        ),
        (
            "ws_json",
            Value::text("/api/navi/ws/v2/json/{session_id}/{name}"),
        ),
        (
            "ws_image",
            Value::text("/api/navi/ws/v2/image/{session_id}/{name}"),
        ),
        (
            "ws_render",
            Value::text("/api/navi/ws/v2/render/{session_id}/{name}"),
        ),
    ])
}

#[path = "handshake.rs"]
mod handshake;
pub(super) use handshake::handshake_error;

async fn handle(
    mut request: Request<Incoming>,
    shared: Shared,
    address: SocketAddr,
) -> Result<Response<Body>, Error> {
    let path = request.uri().path();
    let route = route(path);
    let known = matches!(path, "/" | "/health" | "/api/navi/latest") || route.is_some();
    if !known {
        return Ok(text(StatusCode::NOT_FOUND, "404: Not Found"));
    }
    if request.method() != hyper::Method::GET && request.method() != hyper::Method::HEAD {
        let mut result = text(StatusCode::METHOD_NOT_ALLOWED, "405: Method Not Allowed");
        result
            .headers_mut()
            .insert("allow", hyper::header::HeaderValue::from_static("GET,HEAD"));
        return Ok(result);
    }
    if let Some(route) = route {
        if let Some(error) = handshake_error(request.headers()) {
            return Ok(text(StatusCode::BAD_REQUEST, &error));
        }
        let peer = request
            .headers()
            .get("x-forwarded-for")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
            .unwrap_or_else(|| address.ip().to_string());
        let key = request
            .headers()
            .get("sec-websocket-key")
            .ok_or_else(|| Error::value("missing websocket key"))?;
        let accept = derive_accept_key(key.as_bytes());
        let mut result = Response::new(Full::new(Bytes::new()));
        *result.status_mut() = StatusCode::SWITCHING_PROTOCOLS;
        result.headers_mut().insert(
            "upgrade",
            hyper::header::HeaderValue::from_static("websocket"),
        );
        result.headers_mut().insert(
            "connection",
            hyper::header::HeaderValue::from_static("upgrade"),
        );
        result.headers_mut().insert(
            "sec-websocket-accept",
            accept
                .parse()
                .map_err(|_| Error::value("invalid websocket accept"))?,
        );
        let upgraded = hyper::upgrade::on(&mut request);
        let session = Session { route, peer };
        let owner = shared.clone();
        shared.spawn(async move {
            match upgraded.await {
                Ok(upgraded) => {
                    let config = WebSocketConfig::default().max_message_size(Some(8 * 1024 * 1024));
                    let socket = WebSocketStream::from_raw_socket(
                        TokioIo::new(upgraded),
                        Role::Server,
                        Some(config),
                    )
                    .await;
                    session.run(socket, owner).await;
                }
                Err(error) => eprintln!("WebSocket upgrade: {error}"),
            }
        })?;
        return Ok(result);
    }
    let payload = match path {
        "/" => index(),
        "/health" => shared.with(|receiver| receiver.health())?,
        "/api/navi/latest" => shared.with(|receiver| receiver.latest())?,
        _ => return Ok(text(StatusCode::NOT_FOUND, "404: Not Found")),
    };
    Ok(response(
        StatusCode::OK,
        payload.encode()?,
        "application/json; charset=utf-8",
    ))
}

pub async fn serve(
    request: Request<Incoming>,
    shared: Shared,
    address: SocketAddr,
) -> Result<Response<Body>, Infallible> {
    Ok(match handle(request, shared, address).await {
        Ok(response) => response,
        Err(error) => {
            eprintln!("{}: {error}", error.kind);
            text(
                StatusCode::INTERNAL_SERVER_ERROR,
                "500 Internal Server Error\n\nServer got itself in trouble",
            )
        }
    })
}
