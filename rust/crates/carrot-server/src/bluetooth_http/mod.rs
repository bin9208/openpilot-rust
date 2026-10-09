//! Stationary-only HTTP setup from server/features/bluetooth.py; input remains daemon-owned.
mod commands;
mod http;
mod mutate;
mod state;
mod status;

use crate::{Error, Value};
pub use http::{handle, matches};
use openpilot_bluetooth::bluez::{Bluez, SnapshotReader};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

pub struct Service {
    client: tokio::sync::Mutex<Bluez>,
    reader: Mutex<Option<SnapshotReader>>,
    mutation: tokio::sync::Mutex<()>,
    config: PathBuf,
    runtime: PathBuf,
    command: PathBuf,
    timestamp: Option<f64>,
}

impl Service {
    pub fn original() -> Arc<Self> {
        Self::at(
            "/data/carrot/bluetooth.json".into(),
            "/dev/shm/carrot-bluetooth".into(),
            "sudo".into(),
            None,
            None,
        )
    }

    pub fn at(
        config: PathBuf,
        runtime: PathBuf,
        command: PathBuf,
        bus: Option<String>,
        timestamp: Option<f64>,
    ) -> Arc<Self> {
        Arc::new(Self {
            client: tokio::sync::Mutex::new(Bluez::new(bus)),
            reader: Mutex::new(None),
            mutation: tokio::sync::Mutex::new(()),
            config,
            runtime,
            command,
            timestamp,
        })
    }

    fn now(&self) -> f64 {
        self.timestamp.unwrap_or_else(|| {
            let stamp = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
            stamp.tv_sec as f64 + stamp.tv_nsec as f64 / 1e9
        })
    }

    fn cached_reader(&self) -> Result<Option<SnapshotReader>, Error> {
        self.reader
            .lock()
            .map(|reader| reader.clone())
            .map_err(|_| Error::Source("Bluetooth reader lock poisoned".into()))
    }

    fn cache(&self, reader: Option<SnapshotReader>) -> Result<(), Error> {
        *self
            .reader
            .lock()
            .map_err(|_| Error::Source("Bluetooth reader lock poisoned".into()))? = reader;
        Ok(())
    }

    async fn snapshot(&self) -> Result<openpilot_bluetooth::bluez::Snapshot, Failure> {
        let reader = if let Some(reader) = self.cached_reader()? {
            reader
        } else {
            let mut client = self.client.lock().await;
            let reader = client.snapshot_reader().await?;
            self.cache(Some(reader.clone()))?;
            reader
        };
        Ok(reader.snapshot().await?)
    }

    pub async fn shutdown(&self) -> Result<(), Error> {
        let _mutation = self.mutation.lock().await;
        self.cache(None)?;
        self.client
            .lock()
            .await
            .close()
            .await
            .map_err(|error| Error::Source(error.to_string()))
    }
}

#[derive(Debug, thiserror::Error)]
enum Failure {
    #[error("{1}")]
    Http(hyper::StatusCode, String),
    #[error(transparent)]
    Source(#[from] Error),
    #[error(transparent)]
    Bluez(#[from] openpilot_bluetooth::bluez::Error),
    #[error(transparent)]
    Config(#[from] openpilot_bluetooth::ConfigError),
    #[error(transparent)]
    Files(#[from] openpilot_bluetooth::Error),
    #[error(transparent)]
    Json(#[from] openpilot_carrot_navi::Error),
    #[error(transparent)]
    Encoding(#[from] serde_json::Error),
    #[error(transparent)]
    PythonJson(#[from] openpilot_logmessaged::JsonError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

fn bad(message: &str) -> Failure {
    Failure::Http(hyper::StatusCode::BAD_REQUEST, message.into())
}

fn value(value: &impl serde::Serialize) -> Result<Value, Failure> {
    Ok(Value::parse(&serde_json::to_string(value)?)?)
}
