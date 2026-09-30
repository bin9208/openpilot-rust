use crate::{protocol, Result};
use serde_json::Value;
use std::time::Duration;
use ureq::tls::{parse_pem, PemItem, RootCerts, TlsConfig};

pub const GSMA_CI_BUNDLE: &[u8] =
    include_bytes!("../../../../openpilot/system/hardware/tici/gsma_ci_bundle.pem");
pub trait Es9 {
    fn request(
        &mut self,
        address: &str,
        endpoint: &str,
        payload: Value,
        prefix: &str,
    ) -> Result<Value>;
}
pub struct Http {
    agent: ureq::Agent,
    cookies: cookie_store::CookieStore,
}
impl Http {
    pub fn new() -> Result<Self> {
        Self::with_roots(GSMA_CI_BUNDLE, Duration::from_secs(30))
    }
    /// Root injection supports an owned TLS fixture; certificate and hostname verification remain enabled.
    pub fn with_roots(pem: &[u8], timeout: Duration) -> Result<Self> {
        let certs = parse_pem(pem)
            .filter_map(|item| match item {
                Ok(PemItem::Certificate(c)) => Some(Ok(c)),
                Ok(_) => None,
                Err(e) => Some(Err(e)),
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if certs.is_empty() {
            return Err(protocol("No CA certificates"));
        }
        let config = ureq::Agent::config_builder()
            .tls_config(
                TlsConfig::builder()
                    .root_certs(RootCerts::new_with_certs(&certs))
                    .build(),
            )
            .max_redirects(0)
            .max_redirects_will_error(false)
            .http_status_as_error(false)
            .timeout_connect(Some(timeout))
            .build();
        Ok(Self {
            agent: openpilot_http_transport::socket_timeout_agent(config, timeout),
            cookies: cookie_store::CookieStore::default(),
        })
    }
}
impl Es9 for Http {
    fn request(
        &mut self,
        address: &str,
        endpoint: &str,
        payload: Value,
        prefix: &str,
    ) -> Result<Value> {
        let mut url = url::Url::parse(&format!("https://{address}/gsma/rsp2/es9plus/{endpoint}"))?;
        let mut method = ureq::http::Method::POST;
        let mut body = serde_json::to_vec(&payload)?;
        for redirects in 0..=30 {
            let mut request = ureq::http::Request::builder()
                .method(method.clone())
                .uri(url.as_str())
                .header("User-Agent", "gsma-rsp-lpad")
                .header("X-Admin-Protocol", "gsma/rsp/v2.3.0")
                .header("accept", "*/*")
                .header("accept-encoding", "gzip, deflate, br");
            if method == ureq::http::Method::POST {
                request = request.header("Content-Type", "application/json");
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
                request = request.header("Cookie", cookies);
            }
            // ureq 3.4.2 otherwise sends Set-Cookie attributes in the request Cookie header.
            self.agent.cookie_jar_lock().clear();
            let mut response = self.agent.run(request.body(body.as_slice())?)?;
            for value in response.headers().get_all("set-cookie") {
                if let Ok(value) = value.to_str() {
                    let _ = self.cookies.parse(value, &url);
                }
            }
            let status = response.status().as_u16();
            let location = response
                .headers()
                .get("location")
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            let deflate = response
                .headers()
                .get("content-encoding")
                .is_some_and(|v| v.as_bytes().eq_ignore_ascii_case(b"deflate"));
            let mut bytes = response
                .body_mut()
                .with_config()
                .limit(u64::MAX)
                .read_to_vec()?;
            if deflate {
                use std::io::Read;
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
                    return Err(protocol("exceeded 30 redirects"));
                }
                url = url.join(&location)?;
                if matches!(status, 301..=303) {
                    method = ureq::http::Method::GET;
                    body.clear();
                }
                continue;
            }
            if status >= 400 {
                return Err(ureq::Error::StatusCode(status).into());
            }
            let data = if bytes.is_empty() {
                serde_json::json!({})
            } else {
                serde_json::from_slice(&bytes)?
            };
            check_status(&data, prefix)?;
            return Ok(data);
        }
        Err(protocol("redirect limit"))
    }
}
pub fn check_status(data: &Value, prefix: &str) -> Result<()> {
    let status = &data["header"]["functionExecutionStatus"];
    if status["status"] != "Failed" {
        return Ok(());
    }
    let detail = &status["statusCodeData"];
    let reason = detail["reasonCode"].as_str().unwrap_or("unknown");
    let subject = detail["subjectCode"].as_str().unwrap_or("unknown");
    let message = match (reason, subject) {
        ("3.8", "8.2.6") => {
            "This eSIM profile is already installed on another device. Please use a new QR code."
        }
        ("3.8", "8.2.1") => "This eSIM profile has expired. Please request a new QR code.",
        ("3.8", "8.1") => "The SM-DP+ server refused this request.",
        ("3.1", "8.2.6") => "This eSIM profile has been revoked by the carrier.",
        ("3.9", "8.2.6") => "This eSIM profile download has already been completed.",
        ("2.1", "8.8") => "The device is not compatible with this eSIM profile.",
        ("1.2", "8.1") => "The SM-DP+ server is temporarily unavailable. Try again later.",
        _ => {
            return Err(protocol(format!(
                "{prefix} failed: {reason}/{subject} - {}",
                detail["message"].as_str().unwrap_or("unknown")
            )))
        }
    };
    Err(protocol(message))
}
pub fn system_time_valid() -> Result<bool> {
    Ok(openpilot_timed::clock::valid(
        &openpilot_timed::clock::SystemClock,
        std::path::Path::new("/lib/systemd/systemd"),
    )?)
}
pub fn require_time() -> Result<()> {
    if system_time_valid()? {
        Ok(())
    } else {
        Err(protocol(
            "System time is not set; TLS certificate validation requires a valid clock",
        ))
    }
}
