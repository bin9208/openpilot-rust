use super::{
    diagnostic,
    runtime::{Mode, Spec},
    Service,
};
use crate::{
    http::{Body, RequestBody},
    http_response::{json_response, text},
    web_sound_http::handshake,
    Value,
};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{header, Method, Request, Response, StatusCode};
use std::{net::SocketAddr, sync::Arc};
use tokio_tungstenite::tungstenite::handshake::derive_accept_key;

pub fn matches(path: &str) -> bool {
    matches!(
        path,
        "/api/carrot_navi/capabilities"
            | "/api/carrot_navi/status"
            | "/api/carrot_navi/client_diagnostic"
            | "/ws/carrot_navi/state"
            | "/ws/carrot_navi/media"
    )
}
fn capabilities() -> Value {
    Value::object([
        ("ok", Value::Bool(true)),
        ("feature", Value::text("carrotNavi")),
        ("stateProtocolVersion", Value::integer(1)),
        ("mediaProtocolVersion", Value::integer(1)),
        ("codec", Value::text("avc1.42E01E")),
        ("mapStream", Value::text("render:map_main")),
        ("overlayStreams", Value::text("image:*")),
        ("requiresWebCodecs", Value::Bool(false)),
        ("supportsMediaSource", Value::Bool(true)),
        ("supportsWebRTC", Value::Bool(false)),
        ("browserPipeline", Value::text("server-fmp4-v1")),
        ("sessionProtocolVersion", Value::integer(1)),
        (
            "sessionPolicy",
            Value::text("single-viewer-last-entry-wins"),
        ),
        ("sessionTakeoverChannel", Value::text("state")),
        ("sessionBusyCode", Value::text("carrot_navi_busy")),
        (
            "mapProfiles",
            Value::Array(vec![Value::text("default"), Value::text("cavdy_hud")]),
        ),
    ])
}
fn strip(value: &str) -> &str {
    value.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
}
fn response(value: Value, head: bool) -> Response<Body> {
    json_response(StatusCode::OK, value, head, "")
        .unwrap_or_else(|error| crate::http_response::error_response(error.to_string(), head))
}
fn unavailable(head: bool) -> Response<Body> {
    text(
        StatusCode::SERVICE_UNAVAILABLE,
        "Carrot Navi web bridge unavailable",
        head,
    )
}
pub async fn handle(
    mut request: Request<RequestBody>,
    service: Option<Arc<Service>>,
) -> Response<Body> {
    let path = percent_encoding::percent_decode_str(request.uri().path())
        .decode_utf8_lossy()
        .into_owned();
    let head = request.method() == Method::HEAD;
    let diagnostic = path == "/api/carrot_navi/client_diagnostic";
    if if diagnostic {
        request.method() != Method::POST
    } else {
        !matches!(request.method(), &Method::GET | &Method::HEAD)
    } {
        let mut result = text(
            StatusCode::METHOD_NOT_ALLOWED,
            "405: Method Not Allowed",
            head,
        );
        result.headers_mut().insert(
            header::ALLOW,
            header::HeaderValue::from_static(if diagnostic { "POST" } else { "GET,HEAD" }),
        );
        return result;
    }
    if path == "/api/carrot_navi/capabilities" {
        return response(capabilities(), head);
    }
    let peer = request
        .extensions()
        .get::<SocketAddr>()
        .map_or_else(|| "-".into(), |peer| peer.ip().to_string());
    if diagnostic {
        let mut bytes = Vec::new();
        while let Some(frame) = request.body_mut().frame().await {
            let frame = match frame {
                Ok(frame) => frame,
                Err(error) => return crate::http_response::error_response(error.to_string(), head),
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
                        head,
                    );
                }
                bytes.extend_from_slice(&data);
            }
        }
        if bytes.len() > 8192 {
            return text(
                StatusCode::PAYLOAD_TOO_LARGE,
                &format!(
                    "Maximum request body size 8192 exceeded, actual body size {}",
                    bytes.len()
                ),
                head,
            );
        }
        let value = match diagnostic::parse(&bytes) {
            Ok(value) => value,
            Err(()) => {
                return text(
                    StatusCode::BAD_REQUEST,
                    "invalid Carrot Navi client diagnostic",
                    head,
                )
            }
        };
        if !matches!(value, Value::Object(_)) {
            return text(
                StatusCode::BAD_REQUEST,
                "Carrot Navi client diagnostic must be an object",
                head,
            );
        }
        let Some(service) = service else {
            return unavailable(head);
        };
        if service.diagnostic(peer, value).await.is_err() {
            return unavailable(head);
        }
        return response(Value::object([("ok", Value::Bool(true))]), head);
    }
    let Some(service) = service else {
        return unavailable(head);
    };
    if path == "/api/carrot_navi/status" {
        return match service.status().await {
            Ok(value) => {
                let mut output = Value::object([("ok", Value::Bool(true))]);
                if let Value::Object(fields) = value {
                    if let Value::Object(target) = &mut output {
                        target.extend(fields);
                    }
                }
                response(output, head)
            }
            Err(_) => unavailable(head),
        };
    }
    match service.allowed().await {
        Ok(true) => {}
        Ok(false) => {
            return text(
                StatusCode::CONFLICT,
                "Carrot Navi web stream is unavailable while Cluster HUD is active",
                head,
            )
        }
        Err(_) => return unavailable(head),
    }
    let query: Vec<_> =
        url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes()).collect();
    let get = |name| {
        query
            .iter()
            .find(|(key, _)| key == name)
            .map_or("", |(_, value)| value.as_ref())
    };
    let client: String = strip(get("client_id")).chars().take(128).collect();
    let identity = if client.is_empty() {
        format!("remote:{peer}")
    } else {
        format!("client:{client}")
    };
    let mode = if path == "/ws/carrot_navi/media" {
        let map = query
            .iter()
            .find(|(key, _)| key == "map")
            .map_or("1", |(_, value)| value.as_ref());
        Mode::Media {
            include_map: !matches!(map.to_lowercase().as_str(), "0" | "false" | "no"),
            hud: client == "cavdy-navdy" && strip(get("profile")).to_lowercase() == "cavdy_hud",
        }
    } else {
        Mode::State {
            takeover: matches!(
                get("takeover").to_lowercase().as_str(),
                "1" | "true" | "yes"
            ),
        }
    };
    if let Some(error) = handshake::handshake_error(request.headers()) {
        return text(StatusCode::BAD_REQUEST, &error, head);
    }
    let Some(key) = request.headers().get(header::SEC_WEBSOCKET_KEY) else {
        return text(StatusCode::BAD_REQUEST, "Handshake error: None", head);
    };
    let accept = match header::HeaderValue::try_from(derive_accept_key(key.as_bytes())) {
        Ok(value) => value,
        Err(_) => return unavailable(head),
    };
    if service
        .launch(hyper::upgrade::on(&mut request), Spec { identity, mode })
        .await
        .is_err()
    {
        return unavailable(head);
    }
    let mut result = Response::new(Full::new(Bytes::new()));
    *result.status_mut() = StatusCode::SWITCHING_PROTOCOLS;
    result.headers_mut().insert(
        header::UPGRADE,
        header::HeaderValue::from_static("websocket"),
    );
    result.headers_mut().insert(
        header::CONNECTION,
        header::HeaderValue::from_static("upgrade"),
    );
    result
        .headers_mut()
        .insert(header::SEC_WEBSOCKET_ACCEPT, accept);
    result
}
