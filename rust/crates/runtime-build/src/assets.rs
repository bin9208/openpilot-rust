//! Content-addressed build inputs. Preparing an input never activates a runtime.
use crate::{digest_file, regular_file, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::Read,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
    time::Instant,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    pub url: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub schema: u32,
    pub assets: BTreeMap<String, Asset>,
    /// Space to retain in addition to this input's declared download size.
    pub reserve_bytes: u64,
}

#[derive(Debug, Serialize)]
pub struct Receipt {
    pub schema: u32,
    pub sha256: String,
    pub path: PathBuf,
    pub bytes: u64,
    /// HTTP body bytes, excluding headers/TLS overhead. Zero on a verified hit.
    pub transferred_bytes: u64,
    pub stored_bytes: u64,
    pub elapsed_ms: u128,
}

/// Implementations write only to the supplied, private temporary file.
pub trait Download {
    fn fetch(&self, asset: &Asset, destination: &Path) -> Result<u64>;
}

pub struct Curl;
impl Download for Curl {
    fn fetch(&self, asset: &Asset, destination: &Path) -> Result<u64> {
        let output = Command::new("/usr/bin/curl")
            .args([
                "--fail",
                "--silent",
                "--show-error",
                "--location",
                "--proto",
                "=https",
                "--proto-redir",
                "=https",
                "--connect-timeout",
                "15",
                "--max-time",
                "1800",
                "--max-filesize",
                &asset.bytes.to_string(),
                "--write-out",
                "%{size_download}",
                "--output",
            ])
            .arg(destination)
            .arg("--url")
            .arg(&asset.url)
            .env("LC_ALL", "C")
            .output()?;
        if !output.status.success() {
            // Do not include URLs or response bodies in error reports.
            return Err(format!("build input download failed: {}", output.status).into());
        }
        Ok(String::from_utf8(output.stdout)?.trim().parse()?)
    }
}

impl Plan {
    pub fn read(path: &Path) -> Result<Self> {
        let file = regular_file(path)?;
        if file.metadata()?.len() > 1024 * 1024 {
            return Err("asset plan exceeds 1 MiB".into());
        }
        let plan: Self = serde_json::from_reader(file.take(1024 * 1024 + 1))?;
        plan.validate()?;
        Ok(plan)
    }

    pub(crate) fn validate(&self) -> Result<()> {
        if self.schema != 1 || self.assets.is_empty() || self.assets.len() > 1024 {
            return Err("invalid build input plan".into());
        }
        for (name, asset) in &self.assets {
            if name.is_empty()
                || !name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
            {
                return Err("invalid build input name".into());
            }
            asset.validate()?;
        }
        Ok(())
    }
}

impl Asset {
    fn validate(&self) -> Result<()> {
        let authority = self
            .url
            .strip_prefix("https://")
            .unwrap_or("")
            .split(['/', '?', '#'])
            .next()
            .unwrap_or("");
        if self.sha256.len() != 64
            || !self
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || self.bytes == 0
            || self.bytes > 64 * 1024 * 1024 * 1024
            || authority.is_empty()
            || authority.contains('@')
            || self.url.len() > 8192
            || self.url.contains(['\\', '#'])
            || !self.url.bytes().all(|b| b.is_ascii_graphic())
        {
            return Err("invalid build input digest, size or HTTPS URL".into());
        }
        Ok(())
    }

    fn verify(&self, path: &Path) -> Result<()> {
        if regular_file(path)?.metadata()?.len() != self.bytes || digest_file(path)? != self.sha256
        {
            return Err("cached or downloaded build input identity mismatch".into());
        }
        Ok(())
    }
}

fn directory(path: &Path) -> Result<()> {
    match fs::create_dir(path) {
        Ok(()) => (),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(error) => return Err(error.into()),
    }
    if !fs::symlink_metadata(path)?.is_dir() {
        return Err("build input cache directory is not a real directory".into());
    }
    Ok(())
}

pub fn prepare(
    cache: &Path,
    plan: &Plan,
    download: &impl Download,
) -> Result<BTreeMap<String, Receipt>> {
    plan.validate()?;
    // The caller creates the cache parent; no recursive traversal of aliases.
    directory(cache)?;
    let cache = cache.canonicalize()?;
    let objects = cache.join("sha256");
    directory(&objects)?;
    let mut receipts = BTreeMap::new();
    for (name, asset) in &plan.assets {
        let started = Instant::now();
        let object = objects.join(&asset.sha256);
        directory(&object)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .custom_flags(
                (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32,
            )
            .open(object.join("lock"))?;
        if !lock.metadata()?.is_file() {
            return Err("build input lock is not a regular file".into());
        }
        rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive)?;
        let path = object.join("blob");
        let transferred_bytes = match fs::symlink_metadata(&path) {
            Ok(_) => {
                // A corrupt hit is an error, never a reason to trust or replace it.
                asset.verify(&path)?;
                0
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let space = rustix::fs::statvfs(&object)?;
                let available = space
                    .f_bavail
                    .checked_mul(space.f_frsize)
                    .ok_or("available build input space overflow")?;
                let needed = asset
                    .bytes
                    .checked_add(plan.reserve_bytes)
                    .ok_or("required build input space overflow")?;
                if available < needed {
                    return Err(
                        "insufficient space for build input; existing inputs and runtime retained"
                            .into(),
                    );
                }
                let temporary = tempfile::Builder::new()
                    .prefix(".pending-")
                    .tempfile_in(&object)?;
                let transferred = download.fetch(asset, temporary.path())?;
                asset.verify(temporary.path())?;
                if transferred != asset.bytes {
                    return Err("build input transfer byte count mismatch".into());
                }
                temporary
                    .as_file()
                    .set_permissions(fs::Permissions::from_mode(0o444))?;
                temporary.as_file().sync_all()?;
                temporary.persist_noclobber(&path)?;
                File::open(&object)?.sync_all()?;
                transferred
            }
            Err(error) => return Err(error.into()),
        };
        receipts.insert(
            name.clone(),
            Receipt {
                schema: 1,
                sha256: asset.sha256.clone(),
                path,
                bytes: asset.bytes,
                transferred_bytes,
                stored_bytes: if transferred_bytes == 0 {
                    0
                } else {
                    asset.bytes
                },
                elapsed_ms: started.elapsed().as_millis(),
            },
        );
    }
    Ok(receipts)
}
