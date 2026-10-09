use super::body;
use crate::{mapbox_tokens::transport_error, Error, Value};
use std::{collections::HashMap, io::Read, net::SocketAddr, time::Duration};
use ureq::RequestExt;

const TIMEOUT: Duration = Duration::from_millis(3500);
const PATH: &str = "/carrot/api_heartbeat.php";

pub struct Online {
    endpoint: String,
    agent: ureq::Agent,
}

impl Default for Online {
    fn default() -> Self {
        Self::at(format!("https://shind0.synology.me{PATH}"))
    }
}

impl Online {
    fn at(endpoint: String) -> Self {
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .max_redirects_will_error(false)
            .max_idle_connections(0)
            .timeout_global(None)
            .timeout_connect(Some(TIMEOUT))
            .accept("")
            .accept_encoding("identity")
            .user_agent("Python-urllib/3.12")
            .tls_config(
                ureq::tls::TlsConfig::builder()
                    .disable_verification(true)
                    .build(),
            )
            .build();
        Self {
            endpoint,
            agent: openpilot_http_transport::socket_timeout_agent(config, TIMEOUT),
        }
    }

    pub fn for_owned_peer(peer: SocketAddr, tls: bool) -> Result<Self, Error> {
        if !peer.ip().is_loopback() {
            return Err(Error::Source(
                "heartbeat fixture peer must be loopback".into(),
            ));
        }
        Ok(Self::at(format!(
            "{}://{peer}{PATH}",
            if tls { "https" } else { "http" }
        )))
    }

    pub(super) fn post(&self, payload: &Value) -> Result<(bool, String), Error> {
        let body = payload.encode()?.into_bytes();
        let mut address =
            url::Url::parse(&self.endpoint).map_err(|error| Error::Source(error.to_string()))?;
        let mut method = ureq::http::Method::POST;
        let mut visited: HashMap<String, usize> = HashMap::new();
        loop {
            self.agent.cookie_jar_lock().clear();
            let mut builder = ureq::http::Request::builder()
                .method(method.clone())
                .uri(address.as_str())
                .header("connection", "close");
            let response = if method == ureq::http::Method::POST {
                builder = builder.header("content-type", "application/json");
                builder
                    .body(body.as_slice())
                    .map_err(|error| Error::Source(error.to_string()))?
                    .with_agent(&self.agent)
                    .run()
            } else {
                builder
                    .body(())
                    .map_err(|error| Error::Source(error.to_string()))?
                    .with_agent(&self.agent)
                    .run()
            };
            let mut response = response.map_err(|error| Error::Source(transport_error(error)))?;
            let status = response.status().as_u16();
            let location = response
                .headers()
                .get("location")
                .or_else(|| response.headers().get("uri"))
                .cloned();
            let next = location
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
                });
            let next = next.filter(|next| {
                matches!(next.scheme(), "http" | "https" | "ftp")
                    && visited.get(next.as_str()).copied().unwrap_or_default() < 4
                    && visited.len() < 10
            });
            response.body_mut().track_raw_chunks();
            let mut bytes = Vec::new();
            let read = response.body_mut().as_raw_reader().read_to_end(&mut bytes);
            if let Some(next) = next {
                read.map_err(|error| {
                    Error::Source(body::failure(error, response.body(), bytes.len()))
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
                return Ok((
                    false,
                    format!("HTTPError {status}: {}", String::from_utf8_lossy(&bytes)),
                ));
            }
            if let Err(error) = read {
                let message = body::failure(error, response.body(), bytes.len());
                return Err(Error::Source(message));
            }
            return Ok((true, String::from_utf8_lossy(&bytes).into_owned()));
        }
    }
}
