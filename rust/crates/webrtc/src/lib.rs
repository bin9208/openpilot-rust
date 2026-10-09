//! Project-owned standard and Carrot WebRTC runtime, derived from system/webrtc.
//! Encoded H264 packetization retains the aiortc license and source behavior.

pub mod cereal;
#[cfg(feature = "native")]
mod channel;
#[cfg(feature = "native")]
pub mod network;
#[cfg(feature = "native")]
mod owner_graph;
#[cfg(feature = "native")]
mod peer;
#[cfg(feature = "native")]
mod request;
#[cfg(feature = "native")]
pub mod runtime;
pub mod schema;
#[cfg(feature = "native")]
mod sender;
#[cfg(feature = "native")]
mod session;
pub mod video;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Capnp(#[from] capnp::Error),
    #[error(transparent)]
    Schema(#[from] capnp::NotInSchema),
    #[error(transparent)]
    Utf8(#[from] std::str::Utf8Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    PythonJson(#[from] openpilot_logmessaged::JsonError),
    #[error(transparent)]
    Format(#[from] std::fmt::Error),
    #[error(transparent)]
    Transport(#[from] openpilot_msgq::Error),
    #[error(transparent)]
    Integer(#[from] std::num::TryFromIntError),
    #[error(transparent)]
    Slice(#[from] std::array::TryFromSliceError),
    #[error("WebRTC source contract: {0}")]
    Contract(&'static str),
    #[error("invalid service: {0}")]
    Service(String),
    #[cfg(feature = "native")]
    #[error(transparent)]
    Rtc(#[from] rtc::shared::error::Error),
    #[cfg(feature = "native")]
    #[error(transparent)]
    Crypto(#[from] rtc::crypto::CryptoError),
    #[error("certificate parameters: {0}")]
    Certificate(String),
    #[cfg(feature = "native")]
    #[error(transparent)]
    Interface(#[from] nix::errno::Errno),
    #[cfg(feature = "native")]
    #[error(transparent)]
    Messaging(#[from] openpilot_messaging::runtime::Error),
    #[cfg(feature = "native")]
    #[error(transparent)]
    MessageState(#[from] openpilot_messaging::state::Error),
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error(transparent)]
    Compact(#[from] openpilot_carrot_state::Error),
    #[cfg(feature = "native")]
    #[error(transparent)]
    Codec(#[from] ffmpeg_next::Error),
}
