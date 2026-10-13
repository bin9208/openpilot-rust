use super::{WebDav, WebDavError};
use std::{io::Read, time::Duration};
use ureq::RequestExt;
pub(super) const STREAM_BUFFER: usize = 64 * 1024;

pub(super) fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .allow_non_standard_methods(true)
        .max_redirects(0)
        .max_redirects_will_error(false)
        .proxy(None)
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_send_body(Some(Duration::from_secs(180)))
        .timeout_recv_response(Some(Duration::from_secs(20)))
        .timeout_recv_body(Some(Duration::from_secs(20)))
        .timeout_global(Some(Duration::from_secs(240)))
        .max_idle_connections(0)
        .input_buffer_size(STREAM_BUFFER)
        .output_buffer_size(STREAM_BUFFER)
        .user_agent("openpilot-rust-webdav")
        .build()
        .new_agent()
}

pub(super) fn component(value: &str) -> Result<(), WebDavError> {
    if value.is_empty()
        || matches!(value, "." | "..")
        || value.contains(['/', '\\'])
        || value.chars().any(char::is_control)
    {
        Err(WebDavError::Path)
    } else {
        Ok(())
    }
}

pub(super) struct Request<'a> {
    pub operation: &'static str,
    pub url: &'a url::Url,
    pub headers: &'a [(&'a str, &'a str)],
}
pub(super) enum Body<'a> {
    Empty,
    Stream { reader: &'a mut dyn Read, size: u64 },
}

impl WebDav {
    pub(super) fn url(&self, parts: &[&str], collection: bool) -> Result<url::Url, WebDavError> {
        for part in parts {
            component(part)?;
        }
        let mut encoded = self.base.as_str().to_owned();
        for (index, part) in parts.iter().enumerate() {
            if index > 0 {
                encoded.push('/');
            }
            for byte in part.bytes() {
                if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
                    encoded.push(char::from(byte));
                } else {
                    use std::fmt::Write;
                    write!(encoded, "%{byte:02X}").map_err(|_| WebDavError::Path)?;
                }
            }
        }
        if collection && !parts.is_empty() {
            encoded.push('/');
        }
        url::Url::parse(&encoded).map_err(|_| WebDavError::Path)
    }

    pub(super) fn request(
        &self,
        request: Request<'_>,
        body: Body<'_>,
    ) -> Result<ureq::http::Response<ureq::Body>, WebDavError> {
        let Request {
            operation,
            url,
            headers: extra,
        } = request;
        let size = match &body {
            Body::Empty => 0,
            Body::Stream { size, .. } => *size,
        };
        let mut authorization = ureq::http::HeaderValue::from_str(&self.authorization)
            .map_err(|_| WebDavError::Config)?;
        authorization.set_sensitive(true);
        let mut builder = ureq::http::Request::builder()
            .method(operation)
            .uri(url.as_str())
            .header("Authorization", authorization)
            .header("Content-Length", size);
        for (name, value) in extra {
            builder = builder.header(*name, *value);
        }
        let result = match body {
            Body::Stream { reader, .. } => builder
                .body(ureq::SendBody::from_reader(reader))
                .map_err(|_| WebDavError::Transport)?
                .with_agent(&self.agent)
                .run(),
            Body::Empty => builder
                .body(())
                .map_err(|_| WebDavError::Transport)?
                .with_agent(&self.agent)
                .run(),
        };
        result.map_err(|_| WebDavError::Transport)
    }

    pub(super) fn status(
        operation: &'static str,
        response: &ureq::http::Response<ureq::Body>,
        allowed: &[u16],
    ) -> Result<(), WebDavError> {
        let status = response.status().as_u16();
        if allowed.contains(&status) {
            Ok(())
        } else {
            Err(WebDavError::Status { operation, status })
        }
    }

    /// Checks the selected destination with authenticated OPTIONS, without creating files.
    pub fn health(&self) -> serde_json::Value {
        match self
            .request(
                Request {
                    operation: "OPTIONS",
                    url: &self.base,
                    headers: &[],
                },
                Body::Empty,
            )
            .and_then(|response| {
                Self::status("OPTIONS", &response, &[200, 204])?;
                if !response.headers().contains_key("dav") {
                    return Err(WebDavError::Transport);
                }
                Ok(response.status().as_u16())
            }) {
            Ok(status) => {
                serde_json::json!({"ok":true,"status":status,"target":"webdav","storageOnly":true})
            }
            Err(error) => {
                serde_json::json!({"ok":false,"error":error.to_string(),"target":"webdav","storageOnly":true})
            }
        }
    }
}
