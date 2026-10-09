use super::{ProxyError, MAX_RESPONSE_BYTES};
use crate::request_body::decoder::Decoder;
use hyper::{header, Method, StatusCode};
use std::{
    io::Read,
    net::SocketAddr,
    time::{Duration, Instant},
};
use ureq::RequestExt;

const TOTAL_TIMEOUT: Duration = Duration::from_secs(5);

/// Fixed localhost diagnostic client; no caller URL or runtime endpoint option is accepted.
pub struct Online {
    endpoint: String,
    agent: ureq::Agent,
}

pub(super) struct Upstream {
    pub status: StatusCode,
    pub content_type: Option<header::HeaderValue>,
    pub bytes: Vec<u8>,
}

impl Default for Online {
    fn default() -> Self {
        Self::at("http://127.0.0.1:8082".into())
    }
}

impl Online {
    fn at(endpoint: String) -> Self {
        let config = ureq::Agent::config_builder()
            .proxy(None)
            .http_status_as_error(false)
            .max_redirects(0)
            .max_idle_connections(0)
            .timeout_global(Some(TOTAL_TIMEOUT))
            .timeout_connect(Some(Duration::from_secs(1)))
            .accept_encoding("gzip, deflate, br")
            .build();
        Self {
            endpoint,
            agent: config.new_agent(),
        }
    }

    /// Constructs the dependency seam for an independently owned loopback fixture.
    ///
    /// # Errors
    /// Returns a source error when the fixture address is not loopback.
    pub fn for_owned_peer(peer: SocketAddr) -> Result<Self, crate::Error> {
        if !peer.ip().is_loopback() {
            return Err(crate::Error::Source(
                "Xiaoge fixture peer must be loopback".into(),
            ));
        }
        Ok(Self::at(format!("http://{peer}")))
    }

    pub(super) fn fetch(
        &self,
        method: Method,
        endpoint: &str,
        stream: Option<&str>,
        body: Option<&[u8]>,
    ) -> Result<Upstream, ProxyError> {
        let started = Instant::now();
        let nanos =
            u64::try_from(rustix::time::clock_gettime(rustix::time::ClockId::Monotonic).tv_nsec)
                .map_err(|_| ProxyError::Unavailable)?;
        let timeout = TOTAL_TIMEOUT + Duration::from_nanos(1_000_000_000 - nanos);
        let retry = matches!(method, Method::GET | Method::DELETE);
        let address = format!(
            "{}/api/{endpoint}{}",
            self.endpoint,
            stream.map_or(String::new(), |stream| format!("?stream={stream}"))
        );
        let mut request = ureq::http::Request::builder().method(method).uri(address);
        if body.is_some() {
            request = request.header(header::CONTENT_TYPE, "application/json");
        }
        let request = request
            .body(body.unwrap_or_default())
            .map_err(|_| ProxyError::Unavailable)?;
        let run = || {
            request
                .clone()
                .with_agent(&self.agent)
                .configure()
                .timeout_global(Some(timeout.saturating_sub(started.elapsed())))
                .run()
        };
        let mut response = match run() {
            Err(ureq::Error::Io(error))
                if retry
                    && matches!(
                        error.kind(),
                        std::io::ErrorKind::UnexpectedEof
                            | std::io::ErrorKind::ConnectionReset
                            | std::io::ErrorKind::BrokenPipe
                    ) =>
            {
                run().map_err(transport_error)?
            }
            result => result.map_err(transport_error)?,
        };
        let status = response.status();
        if status.is_redirection() {
            return Err(ProxyError::BadResponse);
        }
        let content_type = response.headers().get(header::CONTENT_TYPE).cloned();
        let initially_buffered = response
            .extensions()
            .get::<ureq::InitialBodyFullyBuffered>()
            .is_some_and(|state| state.0);
        let mut decoding_headers = response.headers().clone();
        if let Some(encoding) = response.extensions().get::<ureq::RawContentEncoding>() {
            decoding_headers.insert(header::CONTENT_ENCODING, encoding.0.clone());
        }
        let mut decoder = Decoder::new(&decoding_headers);
        let mut reader = response.body_mut().as_raw_reader();
        let mut bytes = Vec::new();
        let mut over_limit = false;
        let mut buffer = [0; 65536];
        loop {
            let count = reader.read(&mut buffer).map_err(read_error)?;
            if count == 0 {
                break;
            }
            let decoded = decoder
                .feed(&buffer[..count])
                .map_err(|_| ProxyError::Unavailable)?;
            if bytes.len().saturating_add(decoded.len()) > MAX_RESPONSE_BYTES {
                if !initially_buffered {
                    return Err(ProxyError::BadResponse);
                }
                over_limit = true;
            } else if !over_limit {
                bytes.extend_from_slice(&decoded);
            }
        }
        if decoder.finish().is_err() {
            if initially_buffered {
                return Err(ProxyError::Unavailable);
            }
            std::thread::sleep(timeout.saturating_sub(started.elapsed()));
            return Err(ProxyError::Timeout);
        }
        if over_limit {
            return Err(ProxyError::BadResponse);
        }
        Ok(Upstream {
            status,
            content_type,
            bytes,
        })
    }
}

fn transport_error(error: ureq::Error) -> ProxyError {
    match error {
        ureq::Error::Timeout(ureq::Timeout::Connect) => ProxyError::Unavailable,
        ureq::Error::Timeout(_) => ProxyError::Timeout,
        _ => ProxyError::Unavailable,
    }
}

fn read_error(error: std::io::Error) -> ProxyError {
    if error
        .get_ref()
        .and_then(|error| error.downcast_ref::<ureq::Error>())
        .is_some_and(|error| matches!(error, ureq::Error::Timeout(_)))
    {
        return ProxyError::Timeout;
    }
    if matches!(
        error.kind(),
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
    ) {
        ProxyError::Timeout
    } else {
        ProxyError::Unavailable
    }
}
