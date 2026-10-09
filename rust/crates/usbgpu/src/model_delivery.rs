//! Verified, resumable optional-model delivery; failure does not select a new model.
pub mod archive;
pub mod assets;
pub mod boot;
pub mod catalog;
mod download;
pub mod failure;
pub mod precompiled;
pub mod presence;
pub mod smoke;
mod state;
pub mod status;
pub mod validation;

pub use download::{download, download_fallible, download_observed, sha256, DownloadKind, Event};
pub use state::{
    ensure, ensure_observed, fetch_manifest, parse_manifest, pinned_manifest, State,
    DEFAULT_MANIFEST_URL,
};

use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Http(#[from] ureq::Error),
    #[error(transparent)]
    Native(#[from] crate::Error),
    #[error("{0}")]
    Invalid(String),
}

/// urllib's timeout is a per-socket deadline, including progressive body reads.
pub fn agent(seconds: u64) -> ureq::Agent {
    let native = rustls_native_certs::load_native_certs();
    for error in native.errors {
        eprintln!("model TLS certificate loading: {error}");
    }
    let certificates = native
        .certs
        .iter()
        .map(|cert| ureq::tls::Certificate::from_der(cert.as_ref()).to_owned())
        .collect::<Vec<_>>();
    let tls = ureq::tls::TlsConfig::builder()
        .root_certs(ureq::tls::RootCerts::Specific(certificates.into()))
        .build();
    let config = ureq::Agent::config_builder()
        .tls_config(tls)
        .timeout_connect(Some(Duration::from_secs(seconds)))
        .timeout_global(None)
        .http_status_as_error(false)
        .accept_encoding(ureq::config::AutoHeaderValue::None)
        .max_redirects(10)
        .build();
    openpilot_http_transport::socket_timeout_agent(config, Duration::from_secs(seconds))
}
