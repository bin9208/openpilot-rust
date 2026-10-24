//! GET policy adapted from ui-application/src/api/http.rs (f22c8863b).
//! Repository MIT license / Copyright (c) 2018, Comma.ai, Inc. applies.
//! Returns decompressed network bytes; character decoding is deferred to the caller.
use super::{Get, Response, Result, Transport};
use openpilot_http_transport::socket_timeout_agent;
use std::io::Read;

pub struct NativeTransport;
impl Transport for NativeTransport {
    fn get(&mut self, request: &Get) -> Result<Response> {
        self.send(request, "GET", None)
    }
}
impl NativeTransport {
    pub fn post_form(&mut self, request: &Get, fields: &[(&str, &str)]) -> Result<Response> {
        let body = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(fields.iter().copied())
            .finish();
        self.send(request, "POST", Some(body))
    }
    fn send(&mut self, request: &Get, method: &str, mut body: Option<String>) -> Result<Response> {
        let mut method = method.to_string();
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .max_redirects_will_error(false)
            .timeout_connect(request.timeout)
            .timeout_global(None)
            .user_agent(request.user_agent)
            .build();
        let agent: ureq::Agent = match request.timeout {
            Some(timeout) => socket_timeout_agent(config, timeout),
            None => config.into(),
        };
        let mut cookies = cookie_store::CookieStore::default();
        let mut url = url::Url::parse(&request.url)?;
        let mut authorization = request.authorization.clone();
        for redirects in 0..=30 {
            let mut outgoing = ureq::http::Request::builder()
                .method(method.as_str())
                .uri(url.as_str())
                .header("accept", "*/*")
                .header("accept-encoding", "gzip, deflate, br")
                .header("connection", "keep-alive");
            if let Some(value) = &authorization {
                outgoing = outgoing.header("authorization", value);
            }
            let mut matched = cookies.matches(&url);
            matched.retain(|cookie| cookie.secure() != Some(true) || url.scheme() == "https");
            matched.sort_by_key(|cookie| std::cmp::Reverse(cookie.path.as_ref().len()));
            let cookie_header = matched
                .into_iter()
                .map(|cookie| format!("{}={}", cookie.name(), cookie.value()))
                .collect::<Vec<_>>()
                .join("; ");
            if !cookie_header.is_empty() {
                outgoing = outgoing.header("cookie", cookie_header);
            }
            if body.is_some() {
                outgoing = outgoing.header("content-type", "application/x-www-form-urlencoded");
            }
            // Existing ureq cookie formatting includes attributes: keep its jar empty.
            agent.cookie_jar_lock().clear();
            let mut response =
                agent.run(outgoing.body(body.as_deref().unwrap_or("").as_bytes())?)?;
            for cookie in response.headers().get_all("set-cookie") {
                if let Ok(cookie) = cookie.to_str() {
                    let _ = cookies.parse(cookie, &url);
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
                    return Err("Exceeded 30 redirects".into());
                }
                let next = url.join(&location)?;
                if strip_auth(&url, &next) {
                    authorization = None;
                }
                // Requests rewrites POST for 301/302/303; 307/308 retain its form body.
                if matches!(status, 301 | 302 | 303) {
                    if status != 301 || method == "POST" {
                        method = "GET".into();
                    }
                    body = None;
                }
                url = next;
            } else {
                return Ok(Response {
                    status,
                    body: bytes,
                    content_type,
                });
            }
        }
        Err("Exceeded 30 redirects".into())
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requests_redirect_auth_policy_is_inert_url_comparison_only() {
        for (old, new, strip) in [
            ("https://example.invalid/a", "https://other.invalid/a", true),
            (
                "https://example.invalid/a",
                "https://example.invalid:443/b",
                false,
            ),
            (
                "http://example.invalid/a",
                "https://example.invalid/b",
                false,
            ),
            (
                "https://example.invalid/a",
                "http://example.invalid/b",
                true,
            ),
            (
                "https://example.invalid/a",
                "https://example.invalid:444/a",
                true,
            ),
        ] {
            assert_eq!(
                strip_auth(
                    &url::Url::parse(old).unwrap(),
                    &url::Url::parse(new).unwrap()
                ),
                strip
            );
        }
    }
}
