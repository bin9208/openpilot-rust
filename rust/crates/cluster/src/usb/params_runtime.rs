use super::{packet, Runtime};
use crate::Error;
use chrono::{DateTime, Local, TimeZone};
use num_traits::ToPrimitive;
use openpilot_params::Params;
use openpilot_ui_application::params::store::Store;
use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

#[derive(Default)]
pub struct ParamsRuntime {
    namespace: Namespace,
}

#[derive(Default)]
enum Namespace {
    #[default]
    Runtime,
    Explicit {
        root: PathBuf,
        prefix: String,
    },
}

impl ParamsRuntime {
    /// Use an owned namespace; each source helper constructs its own Params owner.
    #[must_use]
    pub fn at(root: &Path, prefix: &str) -> Self {
        Self {
            namespace: Namespace::Explicit {
                root: root.to_owned(),
                prefix: prefix.to_owned(),
            },
        }
    }

    fn params(&self) -> Result<Store, Error> {
        let params = match &self.namespace {
            Namespace::Runtime => Params::for_runtime(),
            Namespace::Explicit { root, prefix } => Params::open(root, prefix),
        }?;
        Ok(Store::new(params))
    }

    fn boolean(params: &Params, key: &str) -> Result<bool, Error> {
        match params.get(key) {
            Ok(bytes) => Ok(bytes.as_deref() == Some(b"1")),
            // Source C++ read_file reports read/open failures as empty values.
            Err(openpilot_params::Error::Io(_)) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }
}

impl Runtime for ParamsRuntime {
    fn milliseconds(&self) -> Result<u32, Error> {
        let wall = SystemTime::now();
        let local: DateTime<Local> = wall.into();
        let midnight = local
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .ok_or(Error::Contract("invalid local midnight"))?;
        let midnight = Local
            .from_local_datetime(&midnight)
            .earliest()
            .ok_or(Error::Contract("local midnight does not exist"))?;
        let wall = wall
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|_| Error::Contract("USB wall clock precedes Unix epoch"))?
            .as_secs_f64();
        let midnight = midnight
            .timestamp()
            .to_f64()
            .ok_or(Error::Contract("USB midnight timestamp overflow"))?;
        packet::milliseconds(wall, midnight)
    }

    fn sleep(&mut self, duration: Duration) {
        std::thread::sleep(duration);
    }

    fn gpu_active(&self) -> bool {
        match self
            .params()
            .and_then(|params| Self::boolean(&params.raw, "UsbGpuActive"))
        {
            Ok(value) => value,
            Err(error) => {
                eprintln!("Warning: failed to inspect eGPU state for TURZX pacing: {error}");
                false
            }
        }
    }

    fn reset_protected(&self) -> bool {
        match self.params().and_then(|params| {
            let loading = Self::boolean(&params.raw, "UsbGpuLoading")?;
            if loading {
                Ok(true)
            } else {
                Self::boolean(&params.raw, "UsbGpuActive")
            }
        }) {
            Ok(value) => value,
            Err(error) => {
                eprintln!("Warning: failed to inspect eGPU state before USB reset: {error}");
                true
            }
        }
    }

    fn connected(&mut self, value: bool) {
        let params = match self.params() {
            Ok(params) => params,
            Err(error) => {
                eprintln!("Warning: failed to update ClusterHudConnected: {error}");
                return;
            }
        };
        // The source temporary Params destructor waits for its nonblocking writer before this helper returns.
        if let Err(error) = params
            .put_bool_nonblocking("ClusterHudConnected", value)
            .and_then(|()| params.flush())
        {
            eprintln!("Warning: failed to update ClusterHudConnected: {error}");
        }
    }

    fn warning(&mut self, message: &str) {
        eprintln!("{message}");
    }
}
