//! requests-compatible GET sessions for UI API/SSH workers.
use openpilot_http_transport::socket_timeout_agent;
use std::{io::Read, time::Duration};
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Http(#[from] ureq::Error),
    #[error(transparent)]
    Request(#[from] ureq::http::Error),
    #[error(transparent)]
    Url(#[from] url::ParseError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Text(#[from] openpilot_registration::Error),
    #[error("Exceeded 30 redirects")]
    Redirects,
}
impl Error {
    pub fn timed_out(&self) -> bool {
        matches!(self, Self::Http(ureq::Error::Timeout(_)))
    }
}
pub struct Response {
    pub status: u16,
    pub text: String,
}
pub struct Session {
    agent: ureq::Agent,
    cookies: cookie_store::CookieStore,
}
impl Session {
    pub fn new(user_agent: String, timeout: Option<Duration>) -> Self {
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .max_redirects_will_error(false)
            .timeout_connect(timeout)
            .timeout_global(None)
            .user_agent(user_agent)
            .build();
        Self {
            agent: if let Some(timeout) = timeout {
                socket_timeout_agent(config, timeout)
            } else {
                config.into()
            },
            cookies: cookie_store::CookieStore::default(),
        }
    }
    pub fn get(&mut self, address: &str, identity_token: Option<&str>) -> Result<Response, Error> {
        let mut url = url::Url::parse(address)?;
        let mut authorization = identity_token.map(|token| format!("JWT {token}"));
        for redirects in 0..=30 {
            let mut request = ureq::http::Request::builder()
                .method(ureq::http::Method::GET)
                .uri(url.as_str())
                .header("accept", "*/*")
                .header("accept-encoding", "gzip, deflate, br")
                .header("connection", "keep-alive");
            if let Some(authorization) = &authorization {
                request = request.header("authorization", authorization);
            }
            let mut matched = self.cookies.matches(&url);
            matched.retain(|cookie| cookie.secure() != Some(true) || url.scheme() == "https");
            matched.sort_by_key(|cookie| std::cmp::Reverse(cookie.path.as_ref().len()));
            let cookies = matched
                .into_iter()
                .map(|cookie| format!("{}={}", cookie.name(), cookie.value()))
                .collect::<Vec<_>>()
                .join("; ");
            if !cookies.is_empty() {
                request = request.header("cookie", cookies);
            }
            self.agent.cookie_jar_lock().clear();
            let mut response = self.agent.run(request.body(())?)?;
            for cookie in response.headers().get_all("set-cookie") {
                if let Ok(cookie) = cookie.to_str() {
                    let _ = self.cookies.parse(cookie, &url);
                }
            }
            let status = response.status().as_u16();
            let location = response
                .headers()
                .get("location")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            let content_type = response
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("")
                .to_owned();
            let deflate = response
                .headers()
                .get("content-encoding")
                .is_some_and(|value| value.as_bytes().eq_ignore_ascii_case(b"deflate"));
            let mut bytes = Vec::new();
            response.body_mut().as_reader().read_to_end(&mut bytes)?;
            if deflate {
                let mut decoded = Vec::new();
                if flate2::read::ZlibDecoder::new(bytes.as_slice())
                    .read_to_end(&mut decoded)
                    .is_err()
                {
                    decoded.clear();
                    flate2::read::DeflateDecoder::new(bytes.as_slice())
                        .read_to_end(&mut decoded)?;
                }
                bytes = decoded;
            }
            if let Some(location) =
                location.filter(|_| matches!(status, 301 | 302 | 303 | 307 | 308))
            {
                if redirects == 30 {
                    return Err(Error::Redirects);
                }
                let next = url.join(&location)?;
                if strip_auth(&url, &next) {
                    authorization = None;
                }
                url = next;
            } else {
                return Ok(Response {
                    status,
                    text: openpilot_registration::response_text(&bytes, &content_type)?,
                });
            }
        }
        Err(Error::Redirects)
    }
}
fn strip_auth(old: &url::Url, new: &url::Url) -> bool {
    if old.host_str() != new.host_str() {
        return true;
    }
    if old.scheme() == "http"
        && old.port_or_known_default() == Some(80)
        && new.scheme() == "https"
        && new.port_or_known_default() == Some(443)
    {
        return false;
    }
    old.scheme() != new.scheme() || old.port_or_known_default() != new.port_or_known_default()
}
