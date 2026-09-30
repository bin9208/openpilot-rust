use crate::Error;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Partition {
    pub name: String,
    #[serde(deserialize_with = "optional_size")]
    pub size: Option<u64>,
    #[serde(deserialize_with = "optional_string")]
    pub hash_raw: Option<String>,
    #[serde(default)]
    pub hash: String,
    pub full_check: bool,
    #[serde(default)]
    pub sparse: bool,
    #[serde(default)]
    pub url: String,
    #[serde(default = "yes")]
    pub has_ab: bool,
    #[serde(default, deserialize_with = "optional_string")]
    pub compressed_hash: Option<String>,
    #[serde(default, deserialize_with = "optional_size")]
    pub compressed_size: Option<u64>,
    pub casync_caibx: Option<String>,
    pub casync_store: Option<String>,
}
fn yes() -> bool {
    true
}
fn optional_string<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    Ok(serde_json::Value::deserialize(deserializer)?
        .as_str()
        .map(str::to_owned))
}
fn optional_size<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<u64>, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(value.as_u64().or_else(|| value.as_bool().map(u64::from)))
}
impl Partition {
    pub fn size(&self) -> Result<u64, Error> {
        self.size
            .ok_or_else(|| Error::Contract("invalid partition size".into()))
    }
    pub fn raw_hash(&self) -> Result<&str, Error> {
        self.hash_raw
            .as_deref()
            .ok_or_else(|| Error::Contract("invalid partition hash_raw".into()))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Paths {
    pub cache: PathBuf,
    pub confirmation: PathBuf,
    pub lock: PathBuf,
    pub partitions: PathBuf,
    pub caibx_url: String,
}
impl Default for Paths {
    fn default() -> Self {
        fn env(name: &str, fallback: &str) -> PathBuf {
            std::env::var_os(name).map_or_else(|| fallback.into(), PathBuf::from)
        }
        Self {
            cache: env("AGNOS_DOWNLOAD_CACHE_DIR", "/data/agnos-update-cache"),
            confirmation: env(
                "AGNOS_UPDATE_CONFIRMATION_FILE",
                "/data/agnos-update-confirmed",
            ),
            lock: env("AGNOS_UPDATE_LOCK_FILE", "/tmp/agnos-update.lock"),
            partitions: "/dev/disk/by-partlabel".into(),
            caibx_url: "https://commadist.azureedge.net/agnosupdate/".into(),
        }
    }
}
pub fn load(path: &Path) -> Result<Vec<Partition>, Error> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
pub fn file_checksum(path: &Path) -> Result<String, Error> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        let size = file.read(&mut buffer)?;
        if size == 0 {
            break;
        }
        hash.update(&buffer[..size]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
pub fn download_urls(path: &Path) -> Result<Vec<String>, Error> {
    let manifest: Vec<serde_json::Value> = serde_json::from_slice(&fs::read(path)?)?;
    let mut origins = std::collections::HashSet::new();
    let mut urls = Vec::new();
    for partition in manifest {
        let Some(url) = partition["url"].as_str() else {
            continue;
        };
        let origin = download_origin(url)?;
        if origins.insert(origin) {
            urls.push(url.to_owned());
        }
    }
    Ok(urls)
}
fn download_origin(url: &str) -> Result<(String, String), Error> {
    // urlsplit preserves authority spelling and accepts relative manifest URLs.
    let clean = url
        .trim_start_matches(|c: char| c.is_ascii() && c <= ' ')
        .replace(['\r', '\n', '\t'], "");
    let mut rest = clean.as_str();
    let mut scheme = String::new();
    if let Some((prefix, tail)) = rest.split_once(':') {
        if prefix
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic())
            && prefix
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        {
            scheme = prefix.to_ascii_lowercase();
            rest = tail;
        }
    }
    let authority = rest
        .strip_prefix("//")
        .map(|s| s.split(['/', '?', '#']).next().unwrap_or(""))
        .unwrap_or("");
    if authority.contains('[') != authority.contains(']') {
        return Err(Error::Contract("Invalid IPv6 URL".into()));
    }
    Ok((scheme, authority.into()))
}
pub fn mark_confirmed(paths: &Paths, manifest: &Path) -> Result<(), Error> {
    if let Some(parent) = paths.confirmation.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = PathBuf::from(format!("{}.tmp", paths.confirmation.display()));
    let mut output = File::create(&temporary)?;
    writeln!(output, "{}", file_checksum(manifest)?)?;
    output.sync_all()?;
    fs::rename(temporary, &paths.confirmation)?;
    Ok(())
}
pub fn confirmed(paths: &Paths, manifest: &Path) -> Result<bool, Error> {
    let actual = match fs::read_to_string(&paths.confirmation) {
        Ok(value) => value,
        Err(error) if error.kind() != std::io::ErrorKind::InvalidData => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    match file_checksum(manifest) {
        Ok(expected) => Ok(actual.trim() == expected),
        Err(Error::Io(_)) => Ok(false),
        Err(error) => Err(error),
    }
}
pub fn unlink_if_present(path: &Path) -> Result<(), Error> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}
pub fn acquire_lock(paths: &Paths) -> Result<File, Error> {
    if let Some(parent) = paths.lock.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = File::create(&paths.lock)?;
    match file.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => {
            return Err(Error::Contract(
                "Another AGNOS updater is already running".into(),
            ))
        }
        Err(std::fs::TryLockError::Error(error)) => return Err(error.into()),
    }
    writeln!(file, "{}", std::process::id())?;
    file.flush()?;
    Ok(file)
}
pub fn slot_suffix(slot: u32) -> Result<&'static str, Error> {
    match slot {
        0 => Ok("_a"),
        1 => Ok("_b"),
        _ => Err(Error::Contract("invalid boot slot".into())),
    }
}
pub fn partition_path(paths: &Paths, slot: u32, partition: &Partition) -> Result<PathBuf, Error> {
    Ok(paths.partitions.join(format!(
        "{}{}",
        partition.name,
        if partition.has_ab {
            slot_suffix(slot)?
        } else {
            ""
        }
    )))
}
