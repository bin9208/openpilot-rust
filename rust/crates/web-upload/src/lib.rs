//! Native, blocking counterparts of Carrot's web upload helpers.
//!
//! Call from an upload worker, never the server event loop. This crate does not
//! choose routes, start jobs, read Params, delete files, or select runtime daemons.
#![forbid(unsafe_code)]
mod api;
mod compatibility;
mod connection;
mod folder;
mod http;
mod multipart;
mod response;
mod targets;
mod transport;

pub use api::{
    create_session, create_session_with_purpose, health, post_bytes_socket, post_json_total,
    send_complete,
};
pub use folder::{FolderUpload, Observer, Progress, CHUNK_SIZE};
pub use multipart::TmuxUpload;
pub use openpilot_logging::{Fields, Value};
pub use response::{Response, SessionMode};
pub use targets::{
    api_url, carrot_logs_target, device_id, normalize_base_url, session_payload, tmux_target,
    web_settings, Environment, Target, DEFAULT_TMUX_WEB_UPLOAD_URL, DEFAULT_WEB_UPLOAD_URL,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("web upload URL must start with http:// or https://")]
    InvalidBaseUrl,
    #[error("web upload URL is not configured")]
    MissingBaseUrl,
    #[error("upload session is not configured")]
    MissingSession,
    #[error("upload server did not issue a session")]
    MissingToken,
    #[error("upload canceled")]
    Canceled,
    #[error("invalid upload filename")]
    InvalidFilename,
    #[error("upload filename is not UTF-8")]
    FilenameEncoding,
    #[error("upload file not found: {0}")]
    MissingFile(String),
    #[error("cannot read segment folder: {0}")]
    Folder(std::io::Error),
    #[error("{0}")]
    Source(String),
    #[error("'{0}' object has no attribute 'get'")]
    BodyShape(&'static str),
    #[error("{filename}: {source}")]
    File {
        filename: String,
        source: Box<Error>,
    },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Http(#[from] ureq::Error),
    #[error(transparent)]
    Request(#[from] ureq::http::Error),
    #[error(transparent)]
    Url(#[from] url::ParseError),
    #[error(transparent)]
    Format(#[from] std::fmt::Error),
    #[error("response text decoding failed")]
    Decode,
}
