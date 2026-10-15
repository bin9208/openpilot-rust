use crate::Error;
use serde_json::Value;
use std::fmt;

#[derive(Clone)]
pub struct Registration {
    raw: Value,
    pub device: [u8; 6],
    pub key: [u8; 32],
    pub fps: u32,
    pub quality: u32,
}
impl fmt::Debug for Registration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Registration")
            .field("device", &self.device)
            .field("fps", &self.fps)
            .field("quality", &self.quality)
            .finish_non_exhaustive()
    }
}
fn digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}
/// Python bytes.fromhex allows ASCII whitespace between byte pairs only.
pub fn decode_hex(text: &str) -> Result<Vec<u8>, Error> {
    let mut result = Vec::new();
    let mut bytes = text.bytes();
    while let Some(high) = bytes.next() {
        if matches!(high, b' ' | b'\t' | b'\n' | b'\r' | 11 | 12) {
            continue;
        }
        let high = digit(high).ok_or(Error::InvalidRegistration)?;
        let low = bytes
            .next()
            .and_then(digit)
            .ok_or(Error::InvalidRegistration)?;
        result.push(high * 16 + low);
    }
    Ok(result)
}
impl Registration {
    pub fn parse(text: &str) -> Result<Self, Error> {
        let data: Value = serde_json::from_str(text).map_err(|_| Error::InvalidRegistration)?;
        Self::from_value(data)
    }
    pub fn from_value(data: Value) -> Result<Self, Error> {
        let object = data.as_object().ok_or(Error::InvalidRegistration)?;
        let version = object.get("version").ok_or(Error::InvalidRegistration)?;
        if version != &Value::Bool(true) && version.as_f64() != Some(1.) {
            return Err(Error::InvalidRegistration);
        }
        let text = |name| {
            object
                .get(name)
                .and_then(Value::as_str)
                .ok_or(Error::InvalidRegistration)
        };
        let device = decode_hex(text("device")?)?
            .try_into()
            .map_err(|_| Error::InvalidRegistration)?;
        let key = decode_hex(text("key")?)?
            .try_into()
            .map_err(|_| Error::InvalidRegistration)?;
        let integer = |name, default, low, high| {
            let value = match object.get(name) {
                None => default,
                Some(Value::Number(value)) => value.as_u64().ok_or(Error::InvalidRegistration)?,
                _ => return Err(Error::InvalidRegistration),
            };
            if !(low..=high).contains(&value) {
                return Err(Error::InvalidRegistration);
            }
            u32::try_from(value).map_err(|_| Error::InvalidRegistration)
        };
        let fps = integer("fps", 15, 1, 20)?;
        let quality = integer("quality", 50, 20, 90)?;
        Ok(Self {
            raw: data,
            device,
            key,
            fps,
            quality,
        })
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
}

pub fn config_path() -> std::path::PathBuf {
    let path = std::env::var_os("CARROT_S3_CONFIG").unwrap_or_else(|| crate::DEFAULT_CONFIG.into());
    if path.is_empty() {
        ".".into()
    } else {
        path.into()
    }
}
impl Registration {
    pub fn load(path: &std::path::Path) -> Result<Option<Self>, Error> {
        let path = if path.as_os_str().is_empty() {
            std::path::Path::new(".")
        } else {
            path
        };
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            if path.as_os_str().as_bytes().contains(&0) {
                return Ok(None);
            }
        }
        match std::fs::metadata(path) {
            Ok(_) => {}
            // CPython3.12 pathlib ignores ENOENT/ENOTDIR/EBADF/ELOOP, but
            // propagates permission and other failures instead of losing the
            // saved registration. These are the target Linux errno values.
            Err(error) if matches!(error.raw_os_error(), Some(2 | 20 | 9 | 40)) => {
                return Ok(None);
            }
            Err(error) => return Err(error.into()),
        }
        let text = std::fs::read_to_string(path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::InvalidData {
                Error::InvalidRegistration
            } else {
                error.into()
            }
        })?;
        Self::parse(&text).map(Some)
    }
}

/// Source atomic whitelist replacement; sender never stores hotspot passwords.
pub fn save_config(data: &Value, path: &std::path::Path) -> Result<(), Error> {
    use std::{fs, io::Write};
    let mut fields = Vec::new();
    for name in ["version", "device", "key"] {
        fields.push((name, data.get(name).ok_or(Error::MissingConfigField(name))?));
    }
    for name in ["fps", "quality"] {
        if let Some(value) = data.get(name) {
            fields.push((name, value));
        }
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    fs::create_dir_all(parent)?;
    let mut nonce = [0; 4];
    getrandom::fill(&mut nonce).map_err(|error| std::io::Error::other(error.to_string()))?;
    let suffix: String = nonce.iter().map(|v| format!("{v:02x}")).collect();
    let name = path
        .file_name()
        .ok_or_else(|| std::io::Error::other("registration path has no filename"))?;
    let mut temporary = name.to_os_string();
    temporary.push(format!(".{suffix}.tmp"));
    let temporary = path.with_file_name(temporary);
    let result = (|| {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        // Valid source fields are ASCII hex, integers or version1.0/true.
        // Keep source whitelist order and json.dump's default separators.
        let encoded = fields
            .iter()
            .map(|(name, value)| {
                Ok(format!(
                    "{}: {}",
                    serde_json::to_string(name).map_err(std::io::Error::other)?,
                    serde_json::to_string(value).map_err(std::io::Error::other)?
                ))
            })
            .collect::<Result<Vec<_>, std::io::Error>>()?
            .join(", ");
        file.write_all(format!("{{{encoded}}}").as_bytes())?;
        file.sync_all()?;
        drop(file);
        Registration::load(&temporary)?.ok_or(Error::InvalidRegistration)?;
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    match fs::remove_file(&temporary) {
        Ok(()) => result,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => result,
        Err(error) => Err(error.into()),
    }
}
