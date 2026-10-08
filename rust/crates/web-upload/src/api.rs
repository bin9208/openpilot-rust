use crate::{
    api_url,
    compatibility::{or_empty, strip, truncate, truth},
    http::{auth, Body, Client, Request},
    session_payload, Error, Fields, Response, SessionMode,
};
use std::{io::Cursor, time::Instant};

fn json_request(
    client: &Client,
    url: &str,
    token: &str,
    payload: &Fields,
) -> Result<Response, Error> {
    let mut headers = auth(token);
    headers.insert("Content-Type".into(), "application/json".into());
    client.request(
        Request {
            url,
            method: ureq::http::Method::POST,
            headers,
        },
        Body::Bytes(Cursor::new(payload.to_json()?.into_bytes())),
    )
}
pub fn post_json_total(url: &str, payload: &Fields) -> Result<Response, Error> {
    json_request(&Client::total(), url, "", payload)
}
pub fn post_bytes_socket(
    url: &str,
    content_type: &str,
    bytes: Vec<u8>,
    timeout_seconds: u64,
) -> Result<Response, Error> {
    Client::socket(timeout_seconds).request(
        Request {
            url,
            method: ureq::http::Method::POST,
            headers: [("Content-Type".into(), content_type.into())].into(),
        },
        Body::Bytes(Cursor::new(bytes)),
    )
}
pub fn create_session(base: &str, metadata: &Fields, mode: SessionMode) -> Result<String, Error> {
    create_session_with_purpose(
        base,
        metadata,
        match mode {
            SessionMode::Async => "dashcam",
            SessionMode::Sync => "tmux",
        },
        mode,
    )
}
pub fn create_session_with_purpose(
    base: &str,
    metadata: &Fields,
    purpose: &str,
    mode: SessionMode,
) -> Result<String, Error> {
    let client = match mode {
        SessionMode::Async => Client::total(),
        SessionMode::Sync => Client::socket(12),
    };
    let response = json_request(
        &client,
        &api_url(base, &["session"])?,
        "",
        &session_payload(metadata, purpose)?,
    )?;
    let (text, body) = response.mapping_with_mode(matches!(mode, SessionMode::Sync))?;
    if !(200..300).contains(&response.status) || !body.get("ok").is_some_and(truth) {
        let error = or_empty(body.get("error"))?;
        return Err(Error::Source(format!(
            "upload session HTTP {}: {}",
            response.status,
            truncate(if error.is_empty() { &text } else { &error }, 300)
        )));
    }
    let token = or_empty(body.get("token"))?;
    let token = strip(&token);
    if token.is_empty() {
        Err(Error::MissingToken)
    } else {
        Ok(token.into())
    }
}
pub fn health(base: &str, token: &str) -> serde_json::Value {
    let started = Instant::now();
    let result = (|| {
        let response = Client::total().request(
            Request {
                url: &api_url(base, &["health"])?,
                method: ureq::http::Method::GET,
                headers: auth(token),
            },
            Body::Empty,
        )?;
        let text = response.text()?;
        Ok::<_, Error>(if response.status == 200 {
            serde_json::json!({"ok":true,"status":response.status})
        } else {
            serde_json::json!({"ok":false,"status":response.status,"error":truncate(&text,300)})
        })
    })();
    let mut value =
        result.unwrap_or_else(|error| serde_json::json!({"ok":false,"error":error.to_string()}));
    value["elapsed_ms"] = serde_json::json!(started.elapsed().as_millis());
    value
}
pub fn send_complete(base: &str, token: &str, payload: &Fields) -> serde_json::Value {
    if token.is_empty() {
        return serde_json::json!({"ok":false,"error":Error::MissingSession.to_string()});
    }
    let result = (|| {
        let response = json_request(
            &Client::total(),
            &api_url(base, &["complete"])?,
            token,
            payload,
        )?;
        let text = response.text()?;
        Ok::<_, Error>(if (200..300).contains(&response.status) {
            serde_json::json!({"ok":true,"status":response.status})
        } else {
            serde_json::json!({"ok":false,"status":response.status,"error":truncate(&text,300)})
        })
    })();
    result.unwrap_or_else(|error| serde_json::json!({"ok":false,"error":error.to_string()}))
}
