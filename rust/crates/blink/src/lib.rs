//! Physical-indicator intent: carrot's `tools/jetlink/blink_*.py` worker,
//! ported to run in-process (card for SPAS, the Bluetooth daemon for B1)
//! instead of as a separate Python process.
pub mod display;
pub mod observed;
pub mod policy;
pub mod remote;
pub mod source;
pub mod tbt;
pub mod turn_journey;
pub mod turn_location;
pub mod voice;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum Side {
    Left,
    Right,
}
impl Side {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }
    /// carrot's `atc_type` for a turn this way.
    pub fn turn(self) -> &'static str {
        match self {
            Self::Left => "turn left",
            Self::Right => "turn right",
        }
    }
    /// +1 for left, -1 for right (carrot's steering-angle sign).
    pub fn sign(self) -> f64 {
        match self {
            Self::Left => 1.,
            Self::Right => -1.,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unexpected {0} event")]
    Event(&'static str),
    #[error(transparent)]
    Capnp(#[from] capnp::Error),
    #[error(transparent)]
    Schema(#[from] capnp::NotInSchema),
    #[error(transparent)]
    Utf8(#[from] std::str::Utf8Error),
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error(transparent)]
    Messaging(#[from] openpilot_messaging::state::Error),
}

/// Python `params.get_int`: leading integer, 0 when absent or malformed.
pub fn setting_int(settings: &openpilot_params::Params, key: &str) -> i32 {
    let Ok(Some(bytes)) = settings.get(key) else {
        return 0;
    };
    std::str::from_utf8(&bytes)
        .ok()
        .and_then(|text| text.trim().parse().ok())
        .unwrap_or(0)
}
#[cfg(test)]
mod tbt_tests;
