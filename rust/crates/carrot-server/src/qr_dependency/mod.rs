//! Native-provider replacement for the original params.py Brotli dependency installer.
pub(crate) mod http;
mod install;
mod manifest;
#[cfg(test)]
mod tests;

use crate::{config::Config, static_web::brotli::Brotli, Value};
use std::{
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    sync::Mutex,
};

type Fields = Vec<(Vec<u32>, Value)>;

fn set(fields: &mut Fields, key: &str, value: Value) {
    let key: Vec<u32> = key.chars().map(u32::from).collect();
    if let Some((_, field)) = fields.iter_mut().find(|(name, _)| name == &key) {
        *field = value;
    } else {
        fields.push((key, value));
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Codec(#[from] crate::Error),
    #[error("{0}")]
    Invalid(String),
    #[error("native Brotli encoder/decoder unavailable")]
    Unavailable,
    #[error("Brotli activation at {path} succeeded but directory sync failed: {source}")]
    Activated {
        path: PathBuf,
        source: std::io::Error,
    },
}

pub struct Provider {
    bundle: PathBuf,
    active: PathBuf,
    system_fallback: bool,
    lock: Mutex<()>,
}

impl Provider {
    pub fn original(config: &Config) -> Self {
        Self::new(
            config.repository.join("rust/native/brotli"),
            config
                .state
                .parent()
                .unwrap_or(Path::new("."))
                .join("native-deps/brotli"),
            true,
        )
    }

    pub fn new(bundle: PathBuf, active: PathBuf, system_fallback: bool) -> Self {
        Self {
            bundle,
            active,
            system_fallback,
            lock: Mutex::new(()),
        }
    }

    fn loaded(&self) -> Result<(Brotli, PathBuf), Error> {
        let current = self.active.join("current");
        let (codec, path) = match fs::read_link(&current) {
            Ok(link) => {
                if link.components().count() != 1
                    || !link
                        .as_os_str()
                        .to_string_lossy()
                        .starts_with("generation-")
                {
                    return Err(Error::Invalid("invalid Brotli current generation".into()));
                }
                let root = self.active.join(link);
                if fs::symlink_metadata(&root)?.file_type().is_symlink() {
                    return Err(Error::Invalid(
                        "Brotli generation cannot be a symlink".into(),
                    ));
                }
                let bundle = manifest::Bundle::read(&root)?;
                let codec = Brotli::load_bundle(&root, bundle.manifest.brotli_version)
                    .ok_or(Error::Unavailable)?;
                Ok((codec, root.join(manifest::LIBRARIES[0])))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && self.system_fallback => {
                let codec = Brotli::load().ok_or(Error::Unavailable)?;
                let maps = fs::read_to_string("/proc/self/maps")?;
                let path = maps
                    .lines()
                    .filter_map(|line| line.find('/').map(|offset| &line[offset..]))
                    .find(|path| {
                        Path::new(path).file_name().is_some_and(|name| {
                            name.to_string_lossy().starts_with("libbrotlienc.so.")
                        })
                    })
                    .ok_or(Error::Unavailable)?;
                Ok((codec, PathBuf::from(path)))
            }
            Err(error) => Err(Error::Io(error)),
        }?;
        let bytes = b"Carrot QR Brotli provider";
        if !codec.matches(&codec.compress(bytes)?, bytes) {
            return Err(Error::Unavailable);
        }
        Ok((codec, path))
    }

    pub(crate) fn codec(&self) -> Option<Brotli> {
        self.loaded().ok().map(|(codec, _)| codec)
    }

    pub fn status(&self) -> Value {
        Value::Object(self.status_fields())
    }

    fn status_fields(&self) -> Fields {
        match self.loaded() {
            Ok((_, path)) => self.response(true, &path, None),
            Err(error) => self.response(false, Path::new(""), Some(&error.to_string())),
        }
    }

    fn response(&self, installed: bool, module: &Path, error: Option<&str>) -> Fields {
        let mut value: Fields = [
            ("ok", Value::Bool(true)),
            ("installed", Value::Bool(installed)),
            ("dependency", Value::text("brotli")),
            (
                "format",
                Value::text(if installed { "CQR3" } else { "CQR4" }),
            ),
            ("module_path", Value::text(&module.to_string_lossy())),
            ("target", Value::text(&self.active.to_string_lossy())),
            ("provider", Value::text("native-brotli")),
        ]
        .into_iter()
        .map(|(name, value)| (name.chars().map(u32::from).collect(), value))
        .collect();
        if let Some(error) = error {
            set(&mut value, "error", Value::text(error));
        }
        value
    }

    pub fn ensure(&self) -> Value {
        if self.status().get("installed").truth() {
            return self.result(false, "already installed");
        }
        match self.repair() {
            Ok(configured) => self.result(
                configured,
                if configured {
                    "installed"
                } else {
                    "already installed"
                },
            ),
            Err(error) => {
                let mut status = self.status_fields();
                status.retain(|(name, _)| {
                    name != &"module_path".chars().map(u32::from).collect::<Vec<_>>()
                });
                set(&mut status, "ok", Value::Bool(false));
                set(&mut status, "configured", Value::Bool(false));
                set(&mut status, "error", Value::text(&error.to_string()));
                Value::Object(status)
            }
        }
    }

    fn result(&self, configured: bool, message: &str) -> Value {
        let mut status = self.status_fields();
        set(&mut status, "configured", Value::Bool(configured));
        set(&mut status, "message", Value::text(message));
        Value::Object(status)
    }

    fn repair(&self) -> Result<bool, Error> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| Error::Invalid("Brotli lock poisoned".into()))?;
        fs::create_dir_all(&self.active)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.active.join(".lock"))?;
        rustix::fs::flock(&file, rustix::fs::FlockOperation::LockExclusive)
            .map_err(std::io::Error::from)?;
        if self.status().get("installed").truth() {
            return Ok(false);
        }
        install::install(self)?;
        if !self.status().get("installed").truth() {
            return Err(Error::Unavailable);
        }
        Ok(true)
    }
}
