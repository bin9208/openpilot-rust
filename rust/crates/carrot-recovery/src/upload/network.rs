use crate::{Error, Value};
use std::{collections::HashMap, io::Read, time::Duration};
use ureq::RequestExt;

pub fn failure(error: impl std::fmt::Display) -> Value {
    Value::object([
        ("configured", Value::Bool(true)),
        ("ok", Value::Bool(false)),
        ("error", Value::text(&error.to_string())),
    ])
}

fn transport(error: ureq::Error) -> Error {
    let message = match error {
        ureq::Error::Timeout(_) => "timed out".into(),
        ureq::Error::Io(error) => {
            let detail = match error.raw_os_error() {
                Some(code) => format!(
                    "[Errno {code}] {}",
                    error
                        .to_string()
                        .split(" (os error")
                        .next()
                        .unwrap_or("I/O error")
                ),
                None => error.to_string(),
            };
            format!("<urlopen error {detail}>")
        }
        error => format!("<urlopen error {error}>"),
    };
    Error::Source(message)
}

pub fn post(url: &str, headers: &[(&str, &str)], body: &[u8], timeout: Duration) -> Value {
    request(url, headers, body, timeout, true).unwrap_or_else(failure)
}

pub fn post_no_body(url: &str, headers: &[(&str, &str)], body: &[u8], timeout: Duration) -> Value {
    request(url, headers, body, timeout, false).unwrap_or_else(failure)
}

fn request(
    endpoint: &str,
    headers: &[(&str, &str)],
    body: &[u8],
    timeout: Duration,
    read_success_body: bool,
) -> Result<Value, Error> {
    let roots: ureq::tls::RootCerts = rustls_native_certs::load_native_certs()
        .certs
        .iter()
        .map(|cert| ureq::tls::Certificate::from_der(cert.as_ref()).to_owned())
        .collect::<Vec<_>>()
        .into();
    let config = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .max_redirects(0)
        .max_redirects_will_error(false)
        .max_idle_connections(0)
        .timeout_global(None)
        .timeout_connect(Some(timeout))
        .accept("application/json")
        .accept_encoding("identity")
        .user_agent("CarrotRecovery/2.0")
        .tls_config(ureq::tls::TlsConfig::builder().root_certs(roots).build())
        .build();
    let agent = openpilot_http_transport::socket_timeout_agent(config, timeout);
    let mut address =
        url::Url::parse(endpoint).map_err(|error| Error::Source(error.to_string()))?;
    let mut method = ureq::http::Method::POST;
    let mut visited: HashMap<String, usize> = HashMap::new();
    loop {
        agent.cookie_jar_lock().clear();
        let mut builder = ureq::http::Request::builder()
            .method(method.clone())
            .uri(address.as_str())
            .header("Connection", "close");
        for (name, value) in headers {
            if method == ureq::http::Method::POST
                || !matches!(
                    name.to_ascii_lowercase().as_str(),
                    "content-type" | "content-length"
                )
            {
                builder = builder.header(*name, *value);
            }
        }
        let mut response = if method == ureq::http::Method::POST {
            builder
                .body(body)
                .map_err(|error| Error::Source(error.to_string()))?
                .with_agent(&agent)
                .run()
        } else {
            builder
                .body(())
                .map_err(|error| Error::Source(error.to_string()))?
                .with_agent(&agent)
                .run()
        }
        .map_err(transport)?;
        let status = response.status().as_u16();
        let eligible = matches!(status, 301..=303)
            || (method == ureq::http::Method::GET && matches!(status, 307 | 308));
        let next = response
            .headers()
            .get("location")
            .or_else(|| response.headers().get("uri"))
            .filter(|_| eligible)
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
                matches!(next.scheme(), "http" | "https")
                    && visited.get(next.as_str()).copied().unwrap_or_default() < 4
                    && visited.len() < 10
            });
        if let Some(next) = next {
            std::io::copy(
                &mut response.body_mut().as_raw_reader(),
                &mut std::io::sink(),
            )?;
            *visited.entry(next.to_string()).or_default() += 1;
            method = ureq::http::Method::GET;
            address = next;
            continue;
        }
        let mut bytes = Vec::new();
        let ok = (200..300).contains(&status);
        if ok {
            if !read_success_body {
                return Ok(Value::object([
                    ("configured", Value::Bool(true)),
                    ("ok", Value::Bool(true)),
                    ("status", Value::integer(status)),
                ]));
            }
            response
                .body_mut()
                .as_raw_reader()
                .take(1024 * 1024)
                .read_to_end(&mut bytes)?;
            let parsed = std::str::from_utf8(&bytes)
                .ok()
                .and_then(|text| Value::parse(text).ok())
                .unwrap_or(Value::Null);
            return Ok(Value::object([
                ("configured", Value::Bool(true)),
                ("ok", Value::Bool(true)),
                ("status", Value::integer(status)),
                ("body", parsed),
            ]));
        }
        if response
            .body_mut()
            .as_raw_reader()
            .read_to_end(&mut bytes)
            .is_err()
        {
            bytes.clear();
        }
        let mut detail: String = String::from_utf8_lossy(&bytes).chars().take(500).collect();
        if detail.is_empty() {
            detail = format!(
                "HTTP Error {status}: {}",
                response.status().canonical_reason().unwrap_or("")
            );
        }
        return Ok(Value::object([
            ("configured", Value::Bool(true)),
            ("ok", Value::Bool(false)),
            ("status", Value::integer(status)),
            ("error", Value::text(&detail)),
        ]));
    }
}
