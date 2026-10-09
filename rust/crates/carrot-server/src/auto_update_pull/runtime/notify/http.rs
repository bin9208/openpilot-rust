use crate::Value;
use std::{collections::HashMap, io::Read, time::Duration};
use ureq::RequestExt;

const TIMEOUT: Duration = Duration::from_secs(4);

pub fn post_json(endpoint: &str, payload: &Value) -> (bool, u16, String) {
    let result = send(endpoint, payload);
    match result {
        Ok(result) => result,
        Err(error) => (false, 0, error.to_string()),
    }
}

fn send(endpoint: &str, payload: &Value) -> Result<(bool, u16, String), crate::Error> {
    let config = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .max_redirects(0)
        .max_redirects_will_error(false)
        .max_idle_connections(0)
        .timeout_global(None)
        .timeout_connect(Some(TIMEOUT))
        .accept("")
        .accept_encoding("identity")
        .user_agent("openpilot-cweb-push/1")
        .build();
    let agent = openpilot_http_transport::socket_timeout_agent(config, TIMEOUT);
    let encoded = openpilot_logmessaged::JsonValue::parse(&payload.encode()?)
        .map_err(|error| crate::Error::Source(error.to_string()))?
        .to_json_utf8()
        .map_err(|error| crate::Error::Source(error.to_string()))?;
    let body = crate::state_json::compact_encoded(&encoded).into_bytes();
    let mut address =
        url::Url::parse(endpoint).map_err(|error| crate::Error::Source(error.to_string()))?;
    let mut method = ureq::http::Method::POST;
    let mut visited: HashMap<String, usize> = HashMap::new();
    loop {
        agent.cookie_jar_lock().clear();
        let mut builder = ureq::http::Request::builder()
            .method(method.clone())
            .uri(address.as_str())
            .header("connection", "close");
        let response = if method == ureq::http::Method::POST {
            builder = builder.header("content-type", "application/json");
            builder
                .body(body.as_slice())
                .map_err(|error| crate::Error::Source(error.to_string()))?
                .with_agent(&agent)
                .run()
        } else {
            builder
                .body(())
                .map_err(|error| crate::Error::Source(error.to_string()))?
                .with_agent(&agent)
                .run()
        };
        let mut response = response
            .map_err(|error| crate::Error::Source(crate::mapbox_tokens::transport_error(error)))?;
        let status = response.status().as_u16();
        let next = response
            .headers()
            .get("location")
            .or_else(|| response.headers().get("uri"))
            .filter(|_| matches!(status, 301..=303 | 307 | 308))
            .filter(|_| method != ureq::http::Method::POST || matches!(status, 301..=303))
            .and_then(|location| {
                let encoded: String = location
                    .as_bytes()
                    .iter()
                    .map(|byte| {
                        if byte.is_ascii_alphanumeric() || byte.is_ascii_punctuation() {
                            char::from(*byte).to_string()
                        } else {
                            format!("%{byte:02X}")
                        }
                    })
                    .collect();
                address.join(&encoded).ok()
            })
            .filter(|next| {
                matches!(next.scheme(), "http" | "https" | "ftp")
                    && visited.get(next.as_str()).copied().unwrap_or_default() < 4
                    && visited.len() < 10
            });
        response.body_mut().track_raw_chunks();
        let mut bytes = Vec::new();
        let read = response.body_mut().as_raw_reader().read_to_end(&mut bytes);
        if let Some(next) = next {
            read.map_err(|error| {
                crate::Error::Source(crate::heartbeat::body::failure(
                    error,
                    response.body(),
                    bytes.len(),
                ))
            })?;
            visited
                .entry(next.to_string())
                .and_modify(|count| *count += 1)
                .or_insert(1);
            if matches!(status, 301..=303) {
                method = ureq::http::Method::GET;
            }
            address = next;
            continue;
        }
        if !(200..300).contains(&status) {
            if read.is_err() {
                bytes.clear();
            }
            return Ok((false, status, String::from_utf8_lossy(&bytes).into_owned()));
        }
        read.map_err(|error| {
            crate::Error::Source(crate::heartbeat::body::failure(
                error,
                response.body(),
                bytes.len(),
            ))
        })?;
        return Ok((true, status, String::from_utf8_lossy(&bytes).into_owned()));
    }
}
