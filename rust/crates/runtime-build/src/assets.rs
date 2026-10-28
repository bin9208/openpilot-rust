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
    time::{Duration, Instant},
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
        fetch_with_curl(
            Path::new("/usr/bin/curl"),
            asset,
            destination,
            std::thread::sleep,
        )
    }
}

fn fetch_with_curl(
    curl: &Path,
    asset: &Asset,
    destination: &Path,
    sleep: impl Fn(Duration),
) -> Result<u64> {
    let started = Instant::now();
    for attempt in 0..4 {
        let remaining = 1800_u64.saturating_sub(started.elapsed().as_secs());
        if remaining == 0 {
            return Err("build input download time budget exhausted".into());
        }
        let output = Command::new(curl)
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
                &remaining.to_string(),
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
        let transferred: u64 = String::from_utf8(output.stdout)?.trim().parse()?;
        if output.status.success() {
            return Ok(transferred);
        }
        // Retry only before any body arrived. Partial transfers remain failures,
        // so successful receipts still account for every transferred body byte.
        if transferred != 0
            || attempt == 3
            || !matches!(output.status.code(), Some(5 | 6 | 7 | 28 | 35 | 52 | 56))
        {
            // Do not include URLs or response bodies in error reports.
            return Err(format!("build input download failed: {}", output.status).into());
        }
        sleep(Duration::from_secs(2_u64 << attempt));
    }
    unreachable!("last failed attempt returns above")
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

#[cfg(test)]
mod curl_tests {
    use super::*;
    use std::cell::RefCell;

    fn scenario(failures: usize, exit: i32, body_bytes: u64) -> (bool, usize, Vec<u64>) {
        let temp = tempfile::tempdir().unwrap();
        let curl = temp.path().join("curl");
        let calls = temp.path().join("calls");
        fs::write(
            &curl,
            format!(
                "#!/bin/sh\ncd -- \"$(dirname -- \"$0\")\"\n\
                 printf 'call\\n' >> calls\n\
                 count=$(wc -l < calls)\n\
                 if [ \"$count\" -le {failures} ]; then printf '{body_bytes}'; exit {exit}; fi\n\
                 printf '8'\n"
            ),
        )
        .unwrap();
        fs::set_permissions(&curl, fs::Permissions::from_mode(0o700)).unwrap();
        let sleeps = RefCell::new(Vec::new());
        let asset = Asset {
            url: "https://fixture.invalid/never-contacted".into(),
            sha256: "a".repeat(64),
            bytes: 8,
        };
        let result = fetch_with_curl(&curl, &asset, &temp.path().join("pending"), |duration| {
            sleeps.borrow_mut().push(duration.as_secs());
        });
        if let Ok(bytes) = &result {
            assert_eq!(*bytes, 8);
        }
        let count = fs::read_to_string(calls).unwrap().lines().count();
        (result.is_ok(), count, sleeps.into_inner())
    }

    #[test]
    fn connection_failures_retry_with_bounded_backoff_without_hiding_body_bytes() {
        assert_eq!(scenario(2, 28, 0), (true, 3, vec![2, 4]));
        assert_eq!(scenario(10, 28, 0), (false, 4, vec![2, 4, 8]));
        assert_eq!(scenario(1, 28, 3), (false, 1, vec![]));
        assert_eq!(scenario(1, 22, 0), (false, 1, vec![]));
        assert_eq!(scenario(1, 63, 0), (false, 1, vec![]));
    }
}
