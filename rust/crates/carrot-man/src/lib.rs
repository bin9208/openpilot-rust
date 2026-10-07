//! CarrotMan owner port, derived from openpilot/selfdrive/carrot.
//! Original repository licensing and algorithm provenance are retained.
#![deny(unsafe_op_in_unsafe_fn)]

pub mod curve;
pub mod geometry;
#[cfg(feature = "native-skip-miri")]
pub mod geos;
pub mod ingress;
#[cfg(feature = "native-skip-miri")]
pub mod native;
pub mod navigation;
#[cfg(feature = "native-skip-miri")]
pub mod owner;
#[cfg(feature = "native-skip-miri")]
pub mod route;
pub mod serv;
pub mod sources;
pub mod wire;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error(transparent)]
    Navigation(#[from] openpilot_navd::Error),
    #[error("invalid route geometry")]
    Geometry,
    #[error("GEOS native dependency: {0}")]
    Geos(String),
    #[error("CarrotMan contract: {0}")]
    Contract(&'static str),
    #[error(transparent)]
    Capnp(#[from] capnp::Error),
    #[error(transparent)]
    Messaging(#[from] openpilot_messaging::runtime::Error),
    #[error(transparent)]
    MessageState(#[from] openpilot_messaging::state::Error),
    #[error(transparent)]
    Zmq(#[from] zmq::Error),
    #[error(transparent)]
    Upload(#[from] openpilot_web_upload::Error),
}
