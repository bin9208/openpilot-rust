//! Port of locationd/torqued.py. Original MIT license and source provenance apply.
pub mod buckets;
pub mod estimator;
pub mod history;
mod interpolation;
pub mod loop_state;
#[cfg(feature = "native-skip-miri")]
pub mod numerics;
#[cfg(feature = "native-skip-miri")]
pub mod parameters;
#[cfg(feature = "native-skip-miri")]
pub mod platform;
pub mod random;
pub mod wire;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("torqued contract: {0}")]
    Contract(&'static str),
    #[error("torqued cereal: {0}")]
    Cereal(#[from] capnp::Error),
    #[error("torqued enum: {0}")]
    Enum(#[from] capnp::NotInSchema),
    #[error("torqued text: {0}")]
    Text(#[from] std::str::Utf8Error),
    #[error("torqued I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("torqued Params: {0}")]
    Params(#[from] openpilot_params::Error),
    #[error("torqued state: {0}")]
    State(#[from] openpilot_messaging::state::Error),
    #[error("torqued requires native numerical artifact at {path}: {source}")]
    Artifact {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
    #[error("torqued artifact manifest: {0}")]
    Json(#[from] serde_json::Error),
    #[cfg(feature = "native-skip-miri")]
    #[error("torqued requires the pinned native numerical artifact: {0}")]
    Library(#[from] libloading::Error),
    #[cfg(feature = "native-skip-miri")]
    #[error("torqued IPC: {0}")]
    Messaging(#[from] openpilot_messaging::runtime::Error),
}

pub trait Fit {
    fn estimate(&mut self, points: &[[f64; 3]]) -> Result<[f64; 3], Error>;
}
