use super::{response, text};
use crate::{Error, Value};
use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use std::{collections::HashMap, io::Read, time::Duration};

const DIRECTIONS: &str = "/directions/v5/mapbox/driving-traffic/126.9780,37.5665;126.9790,37.5675";
const QUERY: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

pub struct Online {
    endpoint: String,
}
impl Default for Online {
    fn default() -> Self {
        Self {
            endpoint: "https://api.mapbox.com".into(),
        }
    }
}
impl Online {
    pub fn for_test_endpoint(endpoint: String) -> Self {
        Self { endpoint }
    }

    pub fn validate(&self, token: &Value) -> Result<Value, Error> {
        let token = text::utf8(token)?;
        let query = utf8_percent_encode(&token, QUERY)
            .to_string()
            .replace("%20", "+");
        let address = format!(
            "{}{DIRECTIONS}?access_token={query}&overview=false&steps=false&geometries=geojson",
            self.endpoint
        );
        Ok(match request(&address) {
            Ok((status, bytes)) => response::result(status, &bytes),
            Err(message) => response::failure(None, Value::text(&message)),
        })
    }
}

fn transport_error(error: ureq::Error) -> String {
    match error {
        ureq::Error::Timeout(ureq::Timeout::Connect | ureq::Timeout::Resolve) => {
            "<urlopen error timed out>".into()
        }
        ureq::Error::Timeout(_) => "timed out".into(),
        ureq::Error::HostNotFound => "<urlopen error [Errno -2] Name or service not known>".into(),
        ureq::Error::Io(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
            "Remote end closed connection without response".into()
        }
        ureq::Error::Io(error) => {
            if let Some(number) = error.raw_os_error() {
                let message = error.to_string();
                let message = message.split(" (os error").next().unwrap_or(&message);
                format!("<urlopen error [Errno {number}] {message}>")
            } else {
                error.to_string()
            }
        }
        error => error.to_string(),
    }
}

fn body_error(error: std::io::Error) -> String {
    if matches!(
        error.kind(),
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
    ) {
        return "timed out".into();
    }
    if let Some(ureq::Error::Timeout(_)) = error
        .get_ref()
        .and_then(|error| error.downcast_ref::<ureq::Error>())
    {
        return "timed out".into();
    }
    error.to_string()
}

fn request(address: &str) -> Result<(u16, Vec<u8>), String> {
    let timeout = Duration::from_secs(8);
    let config = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .max_redirects(0)
        .max_redirects_will_error(false)
        .timeout_global(None)
        .timeout_connect(Some(timeout))
        .max_idle_connections(0)
        .accept("")
        .accept_encoding("identity")
        .user_agent("CarrotPilot Mapbox token check")
        .build();
    let agent = openpilot_http_transport::socket_timeout_agent(config, timeout);
    let mut address = url::Url::parse(address).map_err(|error| error.to_string())?;
    let mut visited: HashMap<String, usize> = HashMap::new();
    loop {
        agent.cookie_jar_lock().clear();
        let mut response = agent
            .get(address.as_str())
            .header("connection", "close")
            .call()
            .map_err(transport_error)?;
        let status = response.status().as_u16();
        let chunked = response
            .headers()
            .get("transfer-encoding")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.eq_ignore_ascii_case("chunked"));
        let location = response
            .headers()
            .get("location")
            .or_else(|| response.headers().get("uri"))
            .cloned();
        if let Some(location) = location.filter(|_| matches!(status, 301..=303 | 307 | 308)) {
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
            let next = address.join(&encoded).map_err(|error| error.to_string())?;
            let count = visited.get(next.as_str()).copied().unwrap_or_default();
            if !matches!(next.scheme(), "http" | "https" | "ftp")
                || count >= 4
                || visited.len() >= 10
            {
                let mut bytes = Vec::new();
                if response
                    .body_mut()
                    .as_raw_reader()
                    .take(4096)
                    .read_to_end(&mut bytes)
                    .is_err()
                {
                    bytes.clear();
                }
                return Ok((status, bytes));
            }
            visited.insert(next.to_string(), count + 1);
            let mut discard = std::io::sink();
            std::io::copy(&mut response.body_mut().as_raw_reader(), &mut discard)
                .map_err(body_error)?;
            address = next;
            continue;
        }
        let mut bytes = Vec::new();
        let result = response
            .body_mut()
            .as_raw_reader()
            .take(4096)
            .read_to_end(&mut bytes);
        if let Err(error) = result {
            if error.kind() == std::io::ErrorKind::UnexpectedEof && !chunked {
                return Ok((status, bytes));
            } else if (200..300).contains(&status) {
                return Err(body_error(error));
            }
            bytes.clear();
        }
        return Ok((status, bytes));
    }
}
