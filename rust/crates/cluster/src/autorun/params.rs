use super::{orientation, Encoder};
use openpilot_params::Params;
use openpilot_ui_application::params::{numeric, store::Store};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error(transparent)]
    Store(#[from] openpilot_ui_application::Error),
}

#[derive(Clone, Copy, serde::Serialize)]
pub struct Settings {
    pub hud_mode: i64,
    pub encoder: Encoder,
    pub orientation: Option<u8>,
}

pub struct Reader {
    store: Store,
}

impl Reader {
    #[must_use]
    pub fn new(params: Params) -> Self {
        Self {
            store: Store::new(params),
        }
    }

    fn bytes(&self, key: &'static str) -> Result<Option<Vec<u8>>, Error> {
        match self.store.raw.get(key) {
            Ok(value) => Ok(value),
            Err(openpilot_params::Error::Io(_)) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn integer(&self, key: &'static str) -> Result<i64, Error> {
        if let Some(value) = numeric::integer(self.bytes(key)?.as_deref().unwrap_or_default()) {
            Ok(i64::from(value))
        } else {
            eprintln!("[cluster_autorun] fatal malformed integer Params value: {key}");
            std::process::abort();
        }
    }

    fn read_integer(&self, key: &'static str) -> Option<i64> {
        match self.integer(key) {
            Ok(value) => Some(value),
            Err(error) => {
                eprintln!("[cluster_autorun] failed to read {key}: {error}");
                None
            }
        }
    }

    #[must_use]
    pub fn hud_mode(&self) -> i64 {
        self.read_integer("ClusterHud").unwrap_or(0)
    }

    #[must_use]
    pub fn encoder(&self) -> Encoder {
        let value = self.read_integer("ClusterHudEncoder").unwrap_or(0);
        if !(0..=3).contains(&value) {
            eprintln!("[cluster_autorun] unsupported ClusterHudEncoder={value}; using auto");
        }
        Encoder::from_setting(value)
    }

    #[must_use]
    pub fn orientation(&self) -> Option<u8> {
        self.read_integer("ClusterHudOrientation")
            .and_then(orientation)
    }

    #[must_use]
    pub fn settings(&self) -> Settings {
        Settings {
            hud_mode: self.hud_mode(),
            encoder: self.encoder(),
            orientation: self.orientation(),
        }
    }

    pub(crate) fn boolean(&self, key: &'static str) -> Result<bool, Error> {
        Ok(self.bytes(key)?.as_deref() == Some(b"1"))
    }

    #[must_use]
    pub fn output_allowed(&self) -> bool {
        if self.read_integer("ClusterHudDebug").unwrap_or(0) >= 1 {
            return true;
        }
        match self.boolean("IsOnroad") {
            Ok(value) => value,
            Err(error) => {
                eprintln!("[cluster_autorun] failed to read IsOnroad: {error}");
                false
            }
        }
    }

    /// # Errors
    /// Returns failure to queue the source persistent owner's initial connected-state write.
    pub fn initialize(&self) -> Result<(), Error> {
        Ok(self
            .store
            .put_bool_nonblocking("ClusterHudConnected", false)?)
    }
}
