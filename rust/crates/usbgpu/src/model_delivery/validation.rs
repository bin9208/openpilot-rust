//! Device-local validation receipts bound to the actual native execution provider.
use super::{sha256, Error};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::Path};

#[derive(Serialize, Deserialize)]
pub struct Device {
    pub device: String,
    pub machine_id: String,
    pub os: String,
    pub kernel: String,
    pub machine: String,
}
impl Device {
    pub fn read(root: &Path) -> Result<Self, Error> {
        let device = fs::read_to_string(root.join("sys/firmware/devicetree/base/model"))?;
        Ok(Self {
            device: device
                .trim_matches('\0')
                .split("comma ")
                .last()
                .unwrap_or("")
                .to_owned(),
            machine_id: fs::read_to_string(root.join("etc/machine-id"))?
                .trim()
                .to_owned(),
            os: fs::read_to_string(root.join("VERSION"))?.trim().to_owned(),
            kernel: fs::read_to_string(root.join("proc/sys/kernel/osrelease"))?
                .trim()
                .to_owned(),
            machine: std::env::consts::ARCH.into(),
        })
    }
}

#[must_use]
pub fn cameras(device: &str) -> Vec<[u32; 2]> {
    match device {
        "mici" => vec![[1344, 760]],
        "tici" | "tizi" => vec![[1928, 1208]],
        _ => vec![[1928, 1208], [1344, 760]],
    }
}

pub struct Provider<'a> {
    pub worker: &'a Path,
    pub runner: &'a Path,
    pub assets: &'a Path,
}

/// Unknown device/OS identity disables reuse, preserving the source fallback.
///
/// # Errors
/// Missing or unreadable provider inputs must not issue a reusable receipt.
pub fn key(
    model: &Path,
    device: &Device,
    provider: &Provider<'_>,
) -> Result<Option<String>, Error> {
    if !matches!(device.device.as_str(), "mici" | "tici" | "tizi")
        || device.machine_id.is_empty()
        || device.os.is_empty()
    {
        return Ok(None);
    }
    let root = model
        .parent()
        .ok_or_else(|| Error::Invalid("installed model has no directory".into()))?;
    let catalog: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("installed.json"))?)?;
    let identity = serde_json::json!({"schema":2,"catalog":catalog,"device":device,"camera_sizes":cameras(&device.device),
        "provider":{"kind":"native-usbgpu","worker_sha256":sha256(provider.worker)?,"runner_sha256":sha256(provider.runner)?,
            "assets_manifest_sha256":sha256(&provider.assets.join("manifest.json"))?}});
    Ok(Some(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&identity)?)
    )))
}

pub fn cached(model: &Path, key: Option<&str>) -> bool {
    let Some(key) = key else {
        return false;
    };
    let Some(root) = model.parent() else {
        return false;
    };
    if root.join("rejected").exists() {
        return false;
    }
    let Ok(bytes) = fs::read(root.join("boot_validation.json")) else {
        return false;
    };
    serde_json::from_slice::<serde_json::Value>(&bytes)
        .is_ok_and(|value| value == serde_json::json!({"key":key}))
}

pub fn save(model: &Path, key: Option<&str>) -> Result<(), Error> {
    let Some(key) = key else {
        return Ok(());
    };
    let root = model
        .parent()
        .ok_or_else(|| Error::Invalid("installed model has no directory".into()))?;
    let mut staging = tempfile::Builder::new()
        .prefix("boot-validation-")
        .tempfile_in(root)?;
    serde_json::to_writer(&mut staging, &serde_json::json!({"key":key}))?;
    staging.flush()?;
    staging.as_file().sync_all()?;
    staging
        .persist(root.join("boot_validation.json"))
        .map_err(|error| error.error)?;
    Ok(())
}
