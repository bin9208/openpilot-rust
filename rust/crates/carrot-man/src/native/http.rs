use super::{
    actor::{Action, Handle, Response as OwnerResponse},
    config::Config,
    network,
};
use crate::Error;
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{body::Incoming, service::service_fn, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use openpilot_logmessaged::JsonValue;
use std::{convert::Infallible, net::SocketAddr, sync::atomic::Ordering, thread, time::Duration};

fn reply(status: StatusCode, value: serde_json::Value) -> Response<Full<Bytes>> {
    let mut response = Response::new(Full::new(Bytes::from(value.to_string())));
    *response.status_mut() = status;
    response.headers_mut().insert(
        hyper::header::CONTENT_TYPE,
        hyper::header::HeaderValue::from_static("application/json; charset=utf-8"),
    );
    response
}

async fn handle_request(
    mut request: Request<Incoming>,
    peer: SocketAddr,
    handle: Handle,
) -> Result<Response<Full<Bytes>>, Infallible> {
    let path = request.uri().path().to_owned();
    if matches!(*request.method(), hyper::Method::GET | hyper::Method::HEAD) && path == "/health" {
        let head = request.method() == hyper::Method::HEAD;
        let result = tokio::task::spawn_blocking(move || handle.call(Action::Health)).await;
        let mut response = match result {
            Ok(Ok(OwnerResponse::Health(value))) => reply(StatusCode::OK, value),
            _ => reply(
                StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!({"ok":false,"error":"owner stopped"}),
            ),
        };
        if head {
            *response.body_mut() = Full::new(Bytes::new());
        }
        return Ok(response);
    }
    let navi_path =
        path.starts_with("/api/navi/") && !path[10..].is_empty() && !path[10..].contains('/');
    if request.method() != hyper::Method::POST || !navi_path {
        let status = if navi_path || path == "/health" {
            StatusCode::METHOD_NOT_ALLOWED
        } else {
            StatusCode::NOT_FOUND
        };
        return Ok(reply(
            status,
            serde_json::json!({"error":if status==StatusCode::METHOD_NOT_ALLOWED{"405: Method Not Allowed"}else{"404: Not Found"}}),
        ));
    }
    let version = match percent_encoding::percent_decode_str(&path[10..]).decode_utf8() {
        Ok(s) => s.into_owned(),
        Err(_) => {
            return Ok(reply(
                StatusCode::BAD_REQUEST,
                serde_json::json!({"ok":false,"error":"invalid json: invalid path encoding"}),
            ));
        }
    };
    let content_type = request
        .headers()
        .get(hyper::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let mut body = Vec::new();
    while let Some(frame) = request.body_mut().frame().await {
        match frame {
            Ok(frame) => {
                if let Ok(data) = frame.into_data() {
                    body.extend_from_slice(&data);
                    if body.len() > 16 * 1024 * 1024 {
                        return Ok(reply(
                            StatusCode::BAD_REQUEST,
                            serde_json::json!({"ok":false,"error":"invalid json: Request Entity Too Large"}),
                        ));
                    }
                }
            }
            Err(error) => {
                return Ok(reply(
                    StatusCode::BAD_REQUEST,
                    serde_json::json!({"ok":false,"error":format!("invalid json: {error}")}),
                ));
            }
        }
    }
    let text = match decode(&body, &content_type) {
        Ok(s) => s,
        Err(e) => {
            return Ok(reply(
                StatusCode::BAD_REQUEST,
                serde_json::json!({"ok":false,"error":format!("invalid json: {e}")}),
            ));
        }
    };
    if text.trim().is_empty() {
        return Ok(reply(
            StatusCode::BAD_REQUEST,
            serde_json::json!({"ok":false,"error":"invalid json: empty body"}),
        ));
    }
    let value = match JsonValue::parse(text.trim()) {
        Ok(v) if v.is_object() && v.get("schema").is_none() => v,
        Ok(_) => {
            return Ok(reply(
                StatusCode::BAD_REQUEST,
                serde_json::json!({"ok":false,"error":"invalid json: only legacy Tmap objects are accepted on HTTP"}),
            ));
        }
        Err(error) => {
            return Ok(reply(
                StatusCode::BAD_REQUEST,
                serde_json::json!({"ok":false,"error":format!("invalid json: {error}")}),
            ));
        }
    };
    let response_version = version.clone();
    let result = tokio::task::spawn_blocking(move || {
        handle.call(Action::PeerFallback(
            crate::ingress::peers::Fallback::Http,
            SocketAddr::new(peer.ip(), 7705),
        ))?;
        let session = network::legacy_http_session(peer, &version)?;
        let result = network::dispatch_legacy(&value, peer, &session, Some(&version), &handle);
        if result.is_err() {
            let _ = handle.call(Action::Exception);
        }
        result
    })
    .await;
    Ok(match result {
        Ok(Ok(())) => reply(
            StatusCode::OK,
            serde_json::json!({"ok":true,"tmap_version":response_version}),
        ),
        Ok(Err(error)) => reply(
            StatusCode::INTERNAL_SERVER_ERROR,
            serde_json::json!({"ok":false,"error":error.to_string(),"tmap_version":response_version}),
        ),
        Err(error) => reply(
            StatusCode::INTERNAL_SERVER_ERROR,
            serde_json::json!({"ok":false,"error":error.to_string(),"tmap_version":response_version}),
        ),
    })
}

fn decode(bytes: &[u8], content_type: &str) -> Result<String, Error> {
    let charset = content_type
        .split(';')
        .skip(1)
        .find_map(|part| {
            let (k, v) = part.trim().split_once('=')?;
            k.eq_ignore_ascii_case("charset")
                .then(|| v.trim_matches(['"', '\'', ' ']))
        })
        .unwrap_or("utf-8");
    charset_norm::codecs::decode(bytes, charset, charset_norm::codecs::Errors::Strict)
        .map_err(|_| Error::Contract("request text decoding failed"))
}

pub fn start(config: Config, handle: Handle) -> Result<(), Error> {
    thread::Builder::new()
        .name("carrot-http".into())
        .spawn(move || {
            while !handle.stop.load(Ordering::Relaxed) {
                let result = (|| {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()?;
                    runtime.block_on(async {
                        let listener =
                            tokio::net::TcpListener::bind((config.bind, config.http_port)).await?;
                        while !handle.stop.load(Ordering::Relaxed) {
                            match tokio::time::timeout(
                                Duration::from_millis(100),
                                listener.accept(),
                            )
                            .await
                            {
                                Ok(Ok((stream, peer))) => {
                                    let handle = handle.clone();
                                    tokio::spawn(async move {
                                        let service = service_fn(move |incoming| {
                                            handle_request(incoming, peer, handle.clone())
                                        });
                                        if let Err(error) =
                                            hyper::server::conn::http1::Builder::new()
                                                .serve_connection(TokioIo::new(stream), service)
                                                .await
                                        {
                                            eprintln!("carrot_man HTTP connection: {error}");
                                        }
                                    });
                                }
                                Ok(Err(error)) => return Err(Error::Io(error)),
                                Err(_) => {}
                            }
                        }
                        Ok::<_, Error>(())
                    })
                })();
                if let Err(error) = result {
                    let _ = handle.call(Action::Exception);
                    eprintln!("carrot_man HTTP retry: {error}");
                    thread::sleep(Duration::from_secs(2));
                } else {
                    break;
                }
            }
        })?;
    Ok(())
}
