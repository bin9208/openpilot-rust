//! Atomic optional-model status with the original one-second progress throttle.
use super::Error;
use crate::model::Manifest;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Checking,
    Downloading,
    Verifying,
    Ready,
    WaitingForIgnition,
    Compiling,
    Compiled,
    Error,
}

#[derive(Serialize, Deserialize)]
pub struct Status {
    schema_version: u32,
    pub state: Phase,
    pub started_at: f64,
    pub updated_at: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub downloaded_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

pub fn read(cache: &Path) -> Option<Status> {
    let bytes = fs::read(cache.join("status.json")).ok()?;
    let value: Status = serde_json::from_slice(&bytes).ok()?;
    (value.schema_version == 1).then_some(value)
}

pub struct Values<'a> {
    pub model: Option<&'a Manifest>,
    pub downloaded: Option<u64>,
    pub detail: Option<&'a str>,
}

pub fn write(
    cache: &Path,
    phase: Phase,
    values: Values<'_>,
    now: f64,
    started: Option<f64>,
) -> Result<Status, Error> {
    let started_at = started.unwrap_or_else(|| {
        read(cache)
            .filter(|previous| previous.state == phase && previous.started_at != 0.0)
            .map_or(now, |previous| previous.started_at)
    });
    let value = Status {
        schema_version: 1,
        state: phase,
        started_at,
        updated_at: now,
        model_id: values.model.map(|model| model.model_id.clone()),
        sha256: values.model.map(|model| model.sha256.clone()),
        downloaded_bytes: values.downloaded,
        total_bytes: values.model.map(|model| model.size),
        detail: values.detail.map(str::to_owned),
    };
    fs::create_dir_all(cache)?;
    let mut staging = tempfile::Builder::new()
        .prefix(".status-")
        .suffix(".json")
        .tempfile_in(cache)?;
    serde_json::to_writer(&mut staging, &value)?;
    staging.write_all(b"\n")?;
    staging.flush()?;
    staging.as_file().sync_all()?;
    staging
        .persist(cache.join("status.json"))
        .map_err(|error| error.error)?;
    Ok(value)
}

pub struct Reporter {
    cache: PathBuf,
    last_write: Duration,
    started_at: f64,
}
impl Reporter {
    #[must_use]
    pub fn new(cache: &Path, wall_time: f64) -> Self {
        Self {
            cache: cache.to_path_buf(),
            last_write: Duration::ZERO,
            started_at: wall_time,
        }
    }

    pub fn update(
        &mut self,
        phase: Phase,
        values: Values<'_>,
        force: bool,
        monotonic: Duration,
        wall_time: f64,
    ) -> Result<Option<Status>, Error> {
        if !force && monotonic.saturating_sub(self.last_write) < Duration::from_secs(1) {
            return Ok(None);
        }
        self.last_write = monotonic;
        if phase != Phase::Downloading {
            self.started_at = wall_time;
        }
        write(&self.cache, phase, values, wall_time, Some(self.started_at)).map(Some)
    }
}
