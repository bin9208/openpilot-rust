use crate::{Error, Payload, PostResult};
use reqwest::{header, Client, Method};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

pub struct Http(tokio::runtime::Runtime);
impl Http {
    pub fn new() -> std::io::Result<Self> {
        Ok(Self(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?,
        ))
    }
    pub fn post(&self, url: &str, payload: &Payload, seconds: f64) -> PostResult {
        outcome(self.0.block_on(request(url, payload, seconds)))
    }
    pub fn post_stoppable(
        &self,
        url: &str,
        payload: &Payload,
        seconds: f64,
        stop: &AtomicBool,
    ) -> Result<PostResult, Error> {
        self.0.block_on(async {
            let mut pending = std::pin::pin!(request(url, payload, seconds));
            loop {
                if stop.load(Ordering::Relaxed) {
                    return Err(Error::Stopped);
                }
                if let Ok(result) =
                    tokio::time::timeout(Duration::from_millis(20), pending.as_mut()).await
                {
                    return Ok(outcome(result));
                }
            }
        })
    }
}

fn outcome(result: Result<PostResult, String>) -> PostResult {
    result.unwrap_or_else(|body| PostResult {
        ok: false,
        status: 0,
        body,
    })
}

fn transport_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        "timed out".into()
    } else {
        error.to_string()
    }
}

async fn request(address: &str, payload: &Payload, seconds: f64) -> Result<PostResult, String> {
    let timeout = Duration::try_from_secs_f64(seconds).map_err(|error| error.to_string())?;
    let client = Client::builder()
        .connect_timeout(timeout)
        .read_timeout(timeout)
        .http1_only()
        .no_gzip()
        .no_brotli()
        .no_deflate()
        .no_zstd()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .build()
        .map_err(transport_error)?;
    let mut url = url::Url::parse(address).map_err(|error| error.to_string())?;
    let mut method = Method::POST;
    let body = serde_json::to_vec(payload).map_err(|error| error.to_string())?;
    let mut visited: HashMap<String, usize> = HashMap::new();
    loop {
        let mut request = client
            .request(method.clone(), url.clone())
            .header(header::USER_AGENT, "openpilot-cweb-push/1")
            .header(header::ACCEPT_ENCODING, "identity")
            .header(header::CONNECTION, "close");
        if method == Method::POST {
            request = request
                .header(header::CONTENT_TYPE, "application/json")
                .body(body.clone());
        }
        let request = request.build().map_err(transport_error)?;
        let response = client.execute(request).await.map_err(transport_error)?;
        let status = response.status().as_u16();
        let location = response
            .headers()
            .get(header::LOCATION)
            .or_else(|| response.headers().get("uri"))
            .cloned();
        let bytes = match response.bytes().await {
            Ok(bytes) => bytes.to_vec(),
            Err(_) if !(200..300).contains(&status) => Vec::new(),
            Err(error) => return Err(transport_error(error)),
        };
        let response_body = String::from_utf8_lossy(&bytes).into_owned();
        let eligible =
            matches!(status, 301..=303) || (method == Method::GET && matches!(status, 307 | 308));
        if let Some(location) = location.filter(|_| eligible) {
            let encoded: String = location
                .as_bytes()
                .iter()
                .map(|&byte| {
                    if byte.is_ascii_alphanumeric() || byte.is_ascii_punctuation() {
                        char::from(byte).to_string()
                    } else {
                        format!("%{byte:02X}")
                    }
                })
                .collect();
            let next = url.join(&encoded).map_err(|error| error.to_string())?;
            let count = visited.get(next.as_str()).copied().unwrap_or_default();
            if !matches!(next.scheme(), "http" | "https") || count >= 4 || visited.len() >= 10 {
                return Ok(PostResult {
                    ok: false,
                    status,
                    body: response_body,
                });
            }
            visited.insert(next.to_string(), count + 1);
            url = next;
            method = Method::GET;
        } else {
            return Ok(PostResult {
                ok: (200..300).contains(&status),
                status,
                body: response_body,
            });
        }
    }
}
