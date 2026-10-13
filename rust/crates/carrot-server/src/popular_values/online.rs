use super::config::USER_AGENT;
use crate::{Error, Value};
use std::{
    io::Read,
    sync::Arc,
    time::{Duration, Instant},
};

struct SessionCookies(reqwest::cookie::Jar);

impl reqwest::cookie::CookieStore for SessionCookies {
    fn set_cookies(
        &self,
        headers: &mut dyn Iterator<Item = &reqwest::header::HeaderValue>,
        url: &url::Url,
    ) {
        if matches!(url.host(), Some(url::Host::Domain(_))) {
            reqwest::cookie::CookieStore::set_cookies(&self.0, headers, url);
        }
    }
    fn cookies(&self, url: &url::Url) -> Option<reqwest::header::HeaderValue> {
        if matches!(url.host(), Some(url::Host::Domain(_))) {
            reqwest::cookie::CookieStore::cookies(&self.0, url)
        } else {
            None
        }
    }
}

pub(super) fn client() -> Result<reqwest::Client, Error> {
    reqwest::Client::builder()
        .no_proxy()
        .http1_only()
        .redirect(reqwest::redirect::Policy::none())
        .referer(false)
        .retry(reqwest::retry::never())
        .no_gzip()
        .no_brotli()
        .no_deflate()
        .no_zstd()
        .user_agent(USER_AGENT)
        .cookie_provider(Arc::new(SessionCookies(reqwest::cookie::Jar::default())))
        .build()
        .map_err(|_| Error::Source("popular-values HTTP client initialization failed".into()))
}

fn timeout_duration(timeout: f64) -> Result<Option<Duration>, Error> {
    if timeout.is_infinite() {
        return Ok(None);
    }
    let duration = if timeout >= 5. {
        use num_traits::ToPrimitive;
        let clock = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
        let now = clock
            .tv_sec
            .to_f64()
            .ok_or_else(|| Error::Source("monotonic time overflow".into()))?
            + clock
                .tv_nsec
                .to_f64()
                .ok_or_else(|| Error::Source("monotonic time overflow".into()))?
                / 1e9;
        (now + timeout).ceil() - now
    } else {
        timeout
    };
    Duration::try_from_secs_f64(duration)
        .map(Some)
        .map_err(|_| Error::Source("invalid popular-values timeout".into()))
}

pub(super) struct Request {
    pub(super) url: String,
    pub(super) credentials: (String, String),
    pub(super) timeout: f64,
    pub(super) payload: Option<Value>,
}

pub(super) struct Response {
    pub(super) status: u16,
    pub(super) text: String,
}

pub(super) async fn request(client: &reqwest::Client, input: &Request) -> Result<Response, Error> {
    let timeout = timeout_duration(input.timeout)?;
    let started = Instant::now();
    let mut address = url::Url::parse(&input.url)
        .map_err(|_| Error::Source("invalid popular-values URL".into()))?;
    let mut payload = input.payload.as_ref();
    let (id, secret) = &input.credentials;
    let authenticated = !id.is_empty() && !secret.is_empty();
    for redirect in 0..10 {
        let mut request = if let Some(payload) = payload {
            client
                .post(address.clone())
                .header("content-type", "application/json")
                .body(payload.encode()?.into_bytes())
        } else {
            client.get(address.clone())
        };
        request = request
            .header("accept", "*/*")
            .header("accept-encoding", "gzip, deflate, br");
        if let Some(timeout) = timeout {
            request = request.timeout(
                timeout
                    .checked_sub(started.elapsed())
                    .ok_or_else(|| Error::Source("popular-values HTTP timeout".into()))?,
            );
        }
        if authenticated {
            request = request
                .header("CF-Access-Client-Id", id)
                .header("CF-Access-Client-Secret", secret);
        }
        let response = request
            .send()
            .await
            .map_err(|_| Error::Source("popular-values HTTP request failed".into()))?;
        let status = response.status().as_u16();
        if matches!(status, 301..=303 | 307 | 308) {
            let location = response
                .headers()
                .get("location")
                .or_else(|| response.headers().get("uri"));
            if let Some(location) = location {
                if redirect == 9 {
                    return Err(Error::Source(
                        "popular-values redirect limit exceeded".into(),
                    ));
                }
                let location = location
                    .to_str()
                    .map_err(|_| Error::Source("invalid popular-values redirect".into()))?;
                address = address
                    .join(location)
                    .map_err(|_| Error::Source("invalid popular-values redirect".into()))?;
                if matches!(status, 301..=303) {
                    payload = None;
                }
                continue;
            }
        }
        let headers = response.headers().clone();
        let bytes = response
            .bytes()
            .await
            .map_err(|_| Error::Source("popular-values HTTP body failed".into()))?;
        return decode(status, &headers, &bytes);
    }
    Err(Error::Source(
        "popular-values redirect limit exceeded".into(),
    ))
}

fn decode(
    status: u16,
    headers: &reqwest::header::HeaderMap,
    bytes: &[u8],
) -> Result<Response, Error> {
    let encoding = headers
        .get("content-encoding")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let charset = crate::request_text::encoding(headers);
    let mut decoded = Vec::new();
    let bytes = match encoding.to_lowercase().as_str() {
        "gzip" => {
            flate2::read::GzDecoder::new(bytes).read_to_end(&mut decoded)?;
            decoded.as_slice()
        }
        "deflate" => {
            if flate2::read::ZlibDecoder::new(bytes)
                .read_to_end(&mut decoded)
                .is_err()
            {
                decoded.clear();
                flate2::read::DeflateDecoder::new(bytes).read_to_end(&mut decoded)?;
            }
            decoded.as_slice()
        }
        "br" => {
            brotli_decompressor::Decompressor::new(bytes, 4096).read_to_end(&mut decoded)?;
            decoded.as_slice()
        }
        _ => bytes,
    };
    Ok(Response {
        status,
        text: super::payload::strip(&crate::request_text::decode(bytes, &charset)?).to_owned(),
    })
}
