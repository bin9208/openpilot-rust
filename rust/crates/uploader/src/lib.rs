//! Source-compatible native upload orchestration.
mod diagnostics;
pub use diagnostics::{Event, EventSink};
pub mod http;
pub mod runtime;
mod scan;
use openpilot_logging::log_site;
pub use scan::{clear_locks, Attributes, Candidate, XattrCache};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::Instant,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error(transparent)]
    TypedParams(#[from] openpilot_params_typed::Error),
    #[error(transparent)]
    Messaging(#[from] openpilot_messaging::runtime::Error),
    #[error(transparent)]
    State(#[from] openpilot_messaging::state::Error),
    #[error(transparent)]
    Logging(#[from] openpilot_logging::Error),
    #[error(transparent)]
    Capnp(#[from] capnp::Error),
    #[error("{0}")]
    Configuration(&'static str),
    #[error("uploader setxattr failed before last_exc was initialized: {0}")]
    UninitializedLastException(io::Error),
    #[error("invalid uploaded Content-Length: {0}")]
    ContentLength(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Outcome {
    Idle,
    Success,
    Failure,
}
pub struct Backoff(f64);
impl Default for Backoff {
    fn default() -> Self {
        Self(0.1)
    }
}
impl Backoff {
    pub fn current(&self) -> f64 {
        self.0
    }
    pub fn next(&mut self, outcome: Outcome, offroad: bool, jitter_fraction: f64) -> f64 {
        self.0 = match outcome {
            Outcome::Idle => {
                if offroad {
                    60.0
                } else {
                    5.0
                }
            }
            Outcome::Success => 0.1,
            Outcome::Failure => (self.0 * 2.0).min(120.0),
        };
        self.0 + self.0 * jitter_fraction
    }
}

pub struct UploadResponse {
    pub status: u16,
    pub content_length: String,
}
#[derive(Debug, thiserror::Error)]
pub enum TransferError {
    #[error(transparent)]
    Logging(#[from] openpilot_logging::Error),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Http(#[from] ureq::Error),
    #[error(transparent)]
    Request(#[from] ureq::http::Error),
    #[error(transparent)]
    Url(#[from] url::ParseError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Jwt(#[from] jsonwebtoken::errors::Error),
    #[error(transparent)]
    EcKey(#[from] p256::elliptic_curve::Error),
    #[error(transparent)]
    Pkcs8(#[from] p256::pkcs8::Error),
    #[error(transparent)]
    Utf8(#[from] std::str::Utf8Error),
    #[error(transparent)]
    Clock(#[from] std::time::SystemTimeError),
    #[error(transparent)]
    Format(#[from] std::fmt::Error),
    #[error("{0}")]
    Contract(&'static str),
}
pub trait Transfer {
    fn upload(
        &mut self,
        key: &Path,
        path: &Path,
        events: &mut dyn EventSink,
    ) -> Result<UploadResponse, TransferError>;
}

pub struct Uploader<T, A, S> {
    pub root: PathBuf,
    pub last_filename: PathBuf,
    pub transfer: T,
    pub attributes: A,
    pub events: S,
}
impl<T: Transfer, A: Attributes, S: EventSink> Uploader<T, A, S> {
    pub fn new(root: PathBuf, transfer: T, attributes: A, events: S) -> Self {
        Self {
            root,
            last_filename: PathBuf::new(),
            transfer,
            attributes,
            events,
        }
    }
    pub fn step(
        &mut self,
        network_type: u16,
        metered: bool,
        requested_routes: Option<&str>,
    ) -> Result<Outcome, Error> {
        let Some(mut file) = self.next_file(metered, requested_routes)? else {
            return Ok(Outcome::Idle);
        };
        let bytes = file.key.as_os_str().as_encoded_bytes();
        if bytes.ends_with(b"qlog")
            || bytes.ends_with(b"rlog")
            || (bytes.starts_with(b"boot/") && !bytes.ends_with(b".zst"))
        {
            let mut key = file.key.into_os_string();
            key.push(".zst");
            file.key = key.into();
        }
        self.upload(&file, network_type, metered)
    }
    pub fn upload(
        &mut self,
        file: &Candidate,
        network_type: u16,
        metered: bool,
    ) -> Result<Outcome, Error> {
        let size = match fs::metadata(&file.path) {
            Ok(meta) => meta.len(),
            Err(error) => {
                self.events.emit(Event::exception(
                    log_site!(),
                    "upload: getsize failed",
                    &(&file.path, error),
                ))?;
                return Ok(Outcome::Failure);
            }
        };
        let fields = json!({"key":file.key.to_string_lossy(),"fn":file.path.to_string_lossy(),"sz":size,"network_type":network_type,"metered":metered});
        self.events.emit(Event::Fields {
            site: log_site!(),
            name: "upload_start",
            fields: fields.clone(),
        })?;
        let too_large = match file.name.as_encoded_bytes() {
            b"qlog" => size > 25_000_000,
            b"qcam" => size > 5_000_000,
            _ => false,
        };
        let transferred = size != 0 && !too_large;
        let mut last_exception = None;
        let success = if !transferred {
            if too_large {
                self.events.emit(Event::Fields {
                    site: log_site!(),
                    name: "uploader_too_large",
                    fields: json!({"key":file.key.to_string_lossy(),"fn":file.path.to_string_lossy(),"sz":size}),
                })?;
            }
            true
        } else {
            let start = Instant::now();
            match self
                .transfer
                .upload(&file.key, &file.path, &mut self.events)
            {
                Ok(response) if matches!(response.status, 200 | 201 | 401 | 403 | 412) => {
                    self.last_filename.clone_from(&file.path);
                    let mut fields = fields.clone();
                    let name = if response.status == 412 {
                        "upload_ignored"
                    } else {
                        let length = response
                            .content_length
                            .trim()
                            .parse::<i128>()
                            .map_err(|_| Error::ContentLength(response.content_length))?;
                        fields["content_length"] = json!(length);
                        fields["speed"] =
                            json!(length as f64 / 1e6 / start.elapsed().as_secs_f64());
                        "upload_success"
                    };
                    self.events.emit(Event::Fields {
                        site: log_site!(),
                        name,
                        fields,
                    })?;
                    true
                }
                other => {
                    let mut fields = fields.clone();
                    fields["stat"] = match other {
                        Ok(response) => json!(format!("<Response [{}]>", response.status)),
                        Err(error) => {
                            last_exception = Some(diagnostics::error_details(log_site!(), &error));
                            Value::Null
                        }
                    };
                    fields["exc"] = json!(last_exception);
                    self.events.emit(Event::Fields {
                        site: log_site!(),
                        name: "upload_failed",
                        fields,
                    })?;
                    false
                }
            }
        };
        if success {
            if let Err(error) = self.attributes.mark_uploaded(&file.path) {
                if !transferred {
                    return Err(Error::UninitializedLastException(error));
                }
                self.events.emit(Event::Fields {
                    site: log_site!(),
                    name: "uploader_setxattr_failed",
                    fields: json!({"exc":last_exception,"key":file.key.to_string_lossy(),"fn":file.path.to_string_lossy(),"sz":size}),
                })?;
            }
        }
        Ok(if success {
            Outcome::Success
        } else {
            Outcome::Failure
        })
    }
}
