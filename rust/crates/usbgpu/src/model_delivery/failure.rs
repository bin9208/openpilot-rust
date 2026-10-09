//! Preserve transient artifacts and reject persistent model failures.
use crate::Error;
use serde::Deserialize;
use std::{fs, io::Write, path::Path};

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Detail,
    Timeout,
    BrokenPipe,
}

#[must_use]
pub fn transient(detail: &str, kind: Kind) -> bool {
    if matches!(kind, Kind::Timeout | Kind::BrokenPipe) {
        return true;
    }
    let lower = detail.to_lowercase();
    [
        "pcie link not up",
        "pcie power off failed",
        "pcie power on failed",
        "usb bridge reset failed",
        "read(0xb450",
        "f0 out failed: -1",
        "libusb_open: no such device",
        "amd:0 does not exist",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
        || detail.contains("precompiled eGPU worker timed out")
        || detail.contains("precompiled eGPU worker exited")
}

fn remove(path: &Path) -> Result<(), Error> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub fn record(
    path: &Path,
    detail: &str,
    kind: Kind,
    phase: &str,
    wall_time: f64,
) -> Result<bool, Error> {
    let root = path
        .parent()
        .ok_or(Error::Contract("installed model has no directory"))?;
    remove(&root.join("boot_validation.json"))?;
    let value: serde_json::Value = serde_json::from_slice(&fs::read(root.join("installed.json"))?)?;
    let digest = value["pickle"]["sha256"]
        .as_str()
        .ok_or(Error::Contract("installed model hash missing"))?;
    let rejected = !transient(detail, kind);
    let start = detail
        .char_indices()
        .rev()
        .nth(16383)
        .map_or(0, |(index, _)| index);
    let failure = serde_json::json!({"time":wall_time,"phase":phase,"rejected":rejected,
        "pickle_sha256":digest,"error":&detail[start..]});
    let mut staging = tempfile::Builder::new()
        .prefix(".failure-")
        .tempfile_in(root)?;
    serde_json::to_writer(&mut staging, &failure)?;
    staging.flush()?;
    staging
        .persist(root.join("last_failure.json"))
        .map_err(|error| error.error)?;
    if rejected {
        fs::write(root.join("rejected"), digest)?;
    }
    Ok(rejected)
}
