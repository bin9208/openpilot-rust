use super::{KeyError, Username};
use std::{io::Read, time::Duration};

pub struct Online {
    endpoint: String,
    agent: ureq::Agent,
}
impl Default for Online {
    fn default() -> Self {
        Self::at("https://github.com".into())
    }
}
impl Online {
    fn at(endpoint: String) -> Self {
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .proxy(None)
            .timeout_global(Some(Duration::from_secs(10)))
            .max_redirects(10)
            .accept_encoding("gzip, deflate, br")
            .build();
        Self {
            endpoint,
            agent: config.into(),
        }
    }
    pub fn for_test_endpoint(endpoint: String) -> Self {
        Self::at(endpoint)
    }
    pub fn fetch(&self, username: &Username) -> Result<String, KeyError> {
        let mut response = self
            .agent
            .get(format!("{}/{}.keys", self.endpoint, username.as_str()))
            .call()
            .map_err(|error| {
                if matches!(error, ureq::Error::Timeout(_)) {
                    KeyError::rejected(504, "Request timed out")
                } else {
                    KeyError::rejected(502, error.to_string())
                }
            })?;
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let encoding = headers
            .get(hyper::header::CONTENT_ENCODING)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase();
        let mut raw = Vec::new();
        response
            .body_mut()
            .as_reader()
            .read_to_end(&mut raw)
            .map_err(|error| {
                let timeout = matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) || error
                    .get_ref()
                    .and_then(|error| error.downcast_ref::<ureq::Error>())
                    .is_some_and(|error| matches!(error, ureq::Error::Timeout(_)));
                if let Some(ureq::Error::Decompress(encoding, _)) = error
                    .get_ref()
                    .and_then(|error| error.downcast_ref::<ureq::Error>())
                {
                    let encoding = if *encoding == "brotli" {
                        "br"
                    } else {
                        encoding
                    };
                    return KeyError::rejected(
                        502,
                        format!("400, message:\n  Can not decode content-encoding: {encoding}"),
                    );
                }
                if timeout {
                    KeyError::rejected(504, "Request timed out")
                } else {
                    KeyError::rejected(502, error.to_string())
                }
            })?;
        let bytes = decode_deflate(raw, &encoding)?;
        let charset = crate::request_text::encoding(&headers);
        let text = match crate::request_text::decode(&bytes, &charset) {
            Ok(text) => text.into_owned(),
            Err(crate::Error::Source(message))
                if message.starts_with("unsupported request charset:") =>
            {
                if let Some(encoding) = encoding_rs::Encoding::for_label(charset.as_bytes()) {
                    encoding
                        .decode_without_bom_handling_and_without_replacement(&bytes)
                        .ok_or_else(|| {
                            KeyError::rejected(
                                502,
                                format!("cannot decode response charset: {charset}"),
                            )
                        })?
                        .into_owned()
                } else {
                    crate::request_text::decode(&bytes, "utf-8")?.into_owned()
                }
            }
            Err(error) => return Err(error.into()),
        };
        if status == 404 {
            return Err(KeyError::rejected(
                404,
                format!("Username '{}' doesn't exist on GitHub", username.as_str()),
            ));
        }
        if !(200..300).contains(&status) {
            return Err(KeyError::rejected(
                502,
                format!("GitHub request failed: HTTP {status}"),
            ));
        }
        Ok(text)
    }
}

fn decode_deflate(bytes: Vec<u8>, encoding: &str) -> Result<Vec<u8>, KeyError> {
    if encoding != "deflate" {
        return Ok(bytes);
    }
    let mut output = Vec::new();
    let result = flate2::read::ZlibDecoder::new(bytes.as_slice()).read_to_end(&mut output);
    let result = if result.is_err() {
        output.clear();
        flate2::read::DeflateDecoder::new(bytes.as_slice()).read_to_end(&mut output)
    } else {
        result
    };
    result.map_err(|_| {
        KeyError::rejected(
            502,
            format!("400, message:\n  Can not decode content-encoding: {encoding}"),
        )
    })?;
    Ok(output)
}
