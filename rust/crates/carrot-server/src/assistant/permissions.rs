//! Locally granted bearer permissions bound to one web origin. Port of the
//! grant store of carrot's `server/services/assistant_settings.py`.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Debug, thiserror::Error)]
pub enum Fault {
    #[error("{0}")]
    Permission(String),
    #[error("{0}")]
    Invalid(String),
}

pub fn invalid(text: &str) -> Fault {
    Fault::Invalid(text.into())
}

/// `secrets.token_urlsafe(32)`.
pub fn token() -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut bytes = [0u8; 33];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let _ = std::io::Read::read_exact(&mut f, &mut bytes[..32]);
    }
    let mut text = String::with_capacity(44);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(chunk[1]) << 8) | u32::from(chunk[2]);
        for shift in [18, 12, 6, 0] {
            text.push(char::from(ALPHABET[((n >> shift) & 63) as usize]));
        }
    }
    text.truncate(43);
    text
}

pub fn identity(text: &str) -> bool {
    text.len() == 43 && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

pub fn digest(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

pub struct Permissions {
    path: PathBuf,
    pub grants: BTreeMap<String, String>,
}

impl Permissions {
    pub fn load(path: &Path) -> Result<Self, Fault> {
        let saved: Value = match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| invalid("Invalid voice permission store"))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => json!({"version": 1, "grants": {}}),
            Err(e) => return Err(Fault::Invalid(e.to_string())),
        };
        let grants = saved.get("grants").and_then(Value::as_object);
        if saved.get("version") != Some(&json!(1)) || grants.is_none_or(|g| g.len() > 100) {
            return Err(invalid("Invalid voice permission store"));
        }
        let mut result = BTreeMap::new();
        for (digest, origin) in grants.into_iter().flatten() {
            let Some(origin) = origin.as_str() else {
                return Err(invalid("Invalid voice permission store"));
            };
            if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
                return Err(invalid("Invalid voice permission store"));
            }
            result.insert(digest.clone(), origin.to_owned());
        }
        Ok(Self { path: path.to_owned(), grants: result })
    }

    pub fn save(&mut self, grants: BTreeMap<String, String>) -> Result<(), Fault> {
        use std::{io::Write, os::unix::fs::OpenOptionsExt};
        let io = |e: std::io::Error| Fault::Invalid(e.to_string());
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        let temporary = self.path.with_extension("tmp");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(io)?;
        file.write_all(json!({"version": 1, "grants": grants}).to_string().as_bytes()).map_err(io)?;
        file.sync_all().map_err(io)?;
        std::fs::rename(&temporary, &self.path).map_err(io)?;
        self.grants = grants;
        Ok(())
    }

    pub fn enable(&mut self, origin: &str, stationary: bool) -> Result<String, Fault> {
        if !stationary {
            return Err(invalid("Stop and disengage before enabling voice setting access"));
        }
        if self.grants.len() >= 100 {
            return Err(invalid("Voice permission capacity reached"));
        }
        let token = token();
        let mut grants = self.grants.clone();
        grants.insert(digest(&token), origin.to_owned());
        self.save(grants)?;
        Ok(token)
    }

    pub fn authorize(&self, token: &str, origin: &str) -> Result<String, Fault> {
        let digest = digest(token);
        if self.grants.get(&digest).map(String::as_str) != Some(origin) {
            return Err(Fault::Permission("Enable setting access in this Carrot Web connection first".into()));
        }
        Ok(digest)
    }

    pub fn revoke(&mut self, token: &str, origin: &str) -> Result<String, Fault> {
        let digest = self.authorize(token, origin)?;
        let grants = self.grants.iter().filter(|(k, _)| **k != digest).map(|(k, v)| (k.clone(), v.clone())).collect();
        self.save(grants)?;
        Ok(digest)
    }
}
