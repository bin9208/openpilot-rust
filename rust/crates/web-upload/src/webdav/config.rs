use super::transport::agent;
use base64::Engine;
use std::{fs::File, io::Read, os::unix::fs::MetadataExt, path::Path};
const CONFIG_LIMIT: u64 = 16 * 1024;

/// Redacted failures: credentials, private URLs and remote response bodies are never included.
#[derive(Debug, thiserror::Error)]
pub enum WebDavError {
    #[error("private upload configuration is unreadable or not owner-only")]
    ConfigFile,
    #[error("private upload configuration requires HTTPS, a clean URL and credentials")]
    Config,
    #[error("invalid WebDAV path component")]
    Path,
    #[error("WebDAV {operation} failed: HTTP {status}")]
    Status {
        operation: &'static str,
        status: u16,
    },
    #[error("WebDAV transport failed")]
    Transport,
    #[error("WebDAV source file is unreadable or changed during upload")]
    File,
    #[error("WebDAV partial upload cleanup failed")]
    Cleanup,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PrivateConfig {
    url: String,
    username: String,
    password: String,
}

/// A validated private WebDAV destination. Deliberately has no Debug or Serialize implementation.
pub struct WebDav {
    pub(super) base: url::Url,
    pub(super) authorization: String,
    pub(super) agent: ureq::Agent,
}

impl WebDav {
    /// Loads the private device file. Only an absent file selects the existing API backend.
    ///
    /// # Errors
    /// Rejects insecure permissions, symlinks, malformed configuration and non-HTTPS URLs.
    pub fn load(path: &Path) -> Result<Option<Self>, WebDavError> {
        let fd = match rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::CLOEXEC
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::NONBLOCK,
            rustix::fs::Mode::empty(),
        ) {
            Ok(fd) => fd,
            Err(rustix::io::Errno::NOENT) => return Ok(None),
            Err(_) => return Err(WebDavError::ConfigFile),
        };
        let file = File::from(fd);
        let metadata = file.metadata().map_err(|_| WebDavError::ConfigFile)?;
        if !metadata.is_file()
            || metadata.mode() & 0o777 != 0o600
            || metadata.uid() != rustix::process::geteuid().as_raw()
            || metadata.len() > CONFIG_LIMIT
        {
            return Err(WebDavError::ConfigFile);
        }
        let mut bytes = Vec::new();
        file.take(CONFIG_LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| WebDavError::ConfigFile)?;
        if bytes.len() > usize::try_from(CONFIG_LIMIT).map_err(|_| WebDavError::ConfigFile)? {
            return Err(WebDavError::ConfigFile);
        }
        let config: PrivateConfig =
            serde_json::from_slice(&bytes).map_err(|_| WebDavError::Config)?;
        let mut base = url::Url::parse(&config.url).map_err(|_| WebDavError::Config)?;
        if config.url.chars().any(char::is_control)
            || base.scheme() != "https"
            || base.host_str().is_none()
            || !base.username().is_empty()
            || base.password().is_some()
            || base.query().is_some()
            || base.fragment().is_some()
            || config.username.is_empty()
            || config.username.contains(':')
            || config.username.chars().any(char::is_control)
            || config.password.is_empty()
            || config.password.chars().any(char::is_control)
        {
            return Err(WebDavError::Config);
        }
        let path = format!("{}/", base.path().trim_end_matches('/'));
        base.set_path(&path);
        let authorization = format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD
                .encode(format!("{}:{}", config.username, config.password))
        );
        Ok(Some(Self {
            base,
            authorization,
            agent: agent(),
        }))
    }
}
