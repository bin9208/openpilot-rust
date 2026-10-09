use super::Application;
use crate::{schema, Error};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{body::Incoming, header, Method, Request, Response, StatusCode};
use openpilot_logmessaged::JsonValue;
use std::{convert::Infallible, net::SocketAddr};

pub(super) type Reply = Response<Full<Bytes>>;

pub(super) fn reply(status: StatusCode, text: String, json: bool, cors: bool) -> Reply {
    let mut response = Response::new(Full::new(Bytes::from(text)));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static(if json {
            "application/json; charset=utf-8"
        } else {
            "text/plain; charset=utf-8"
        }),
    );
    if cors {
        for (name, value) in [
            ("Access-Control-Allow-Origin", "*"),
            (
                "Access-Control-Allow-Methods",
                "GET, POST, PUT, DELETE, OPTIONS",
            ),
            (
                "Access-Control-Allow-Headers",
                "Content-Type, Authorization",
            ),
        ] {
            response
                .headers_mut()
                .insert(name, header::HeaderValue::from_static(value));
        }
    }
    response
}

pub(super) fn internal(error: &Error) -> Reply {
    eprintln!("WebRTC HTTP request failed: {error}");
    reply(
        StatusCode::INTERNAL_SERVER_ERROR,
        "500 Internal Server Error\n\nServer got itself in trouble".to_owned(),
        false,
        false,
    )
}

async fn body(request: Request<Incoming>) -> Result<String, Reply> {
    let mut body = request.into_body();
    let mut bytes = Vec::new();
    while let Some(frame) = body.frame().await {
        let frame = frame
            .map_err(|error| reply(StatusCode::BAD_REQUEST, error.to_string(), false, false))?;
        if let Ok(data) = frame.into_data() {
            bytes.extend_from_slice(&data);
            if bytes.len() >= 1_048_576 {
                return Err(reply(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    format!(
                        "Maximum request body size 1048576 exceeded, actual body size {}",
                        bytes.len()
                    ),
                    false,
                    false,
                ));
            }
        }
    }
    String::from_utf8(bytes)
        .map_err(|error| reply(StatusCode::BAD_REQUEST, error.to_string(), false, false))
}

impl Application {
    pub(super) async fn handle(
        &self,
        request: Request<Incoming>,
        remote: SocketAddr,
    ) -> Result<Reply, Infallible> {
        let method = request.method().clone();
        let path = request.uri().path().to_owned();
        if method == Method::OPTIONS {
            let mut response = reply(StatusCode::OK, String::new(), false, true);
            response.headers_mut().insert(
                "Access-Control-Max-Age",
                header::HeaderValue::from_static("86400"),
            );
            return Ok(response);
        }
        let response = match (method, path.as_str()) {
            (Method::POST, "/stream") => match body(request).await {
                Ok(body) => self.stream(&body, remote).await,
                Err(response) => response,
            },
            (Method::POST, "/notify") => match body(request).await {
                Ok(text) => match JsonValue::parse(&text).and_then(|value| {
                    value
                        .to_json()
                        .map_err(|_| openpilot_logmessaged::JsonError::Message)
                }) {
                    Ok(text) => {
                        for session in self.streams.borrow().iter() {
                            if let Ok(mut session) = session.value.try_lock() {
                                session.notify(&text);
                            }
                        }
                        reply(StatusCode::OK, "OK".to_owned(), false, true)
                    }
                    Err(_) => reply(
                        StatusCode::BAD_REQUEST,
                        "Invalid JSON".to_owned(),
                        false,
                        false,
                    ),
                },
                Err(_) => reply(
                    StatusCode::BAD_REQUEST,
                    "Invalid JSON".to_owned(),
                    false,
                    false,
                ),
            },
            (Method::GET | Method::HEAD, "/schema") => {
                let value = request.uri().query().and_then(|query| {
                    query
                        .split('&')
                        .find_map(|part| part.strip_prefix("services="))
                });
                match value {
                    Some(value) => {
                        let value = value.replace('+', " ");
                        let value =
                            percent_encoding::percent_decode_str(&value).decode_utf8_lossy();
                        match schema::services(&value.split(',').collect::<Vec<_>>()) {
                            Ok(value) => reply(StatusCode::OK, value.to_string(), true, true),
                            Err(error) => internal(&error),
                        }
                    }
                    None => internal(&Error::Contract("missing services query")),
                }
            }
            (_, "/schema" | "/stream" | "/notify") => {
                let mut response = reply(
                    StatusCode::METHOD_NOT_ALLOWED,
                    "405: Method Not Allowed".to_owned(),
                    false,
                    false,
                );
                response.headers_mut().insert(
                    header::ALLOW,
                    header::HeaderValue::from_static(if path == "/schema" {
                        "GET,HEAD,OPTIONS"
                    } else {
                        "OPTIONS,POST"
                    }),
                );
                response
            }
            _ => reply(
                StatusCode::NOT_FOUND,
                "404: Not Found".to_owned(),
                false,
                false,
            ),
        };
        Ok(response)
    }
}
