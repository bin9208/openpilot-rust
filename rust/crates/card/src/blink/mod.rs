//! Physical-indicator intent: carrot's `tools/jetlink/blink_*.py` worker,
//! ported to run inside card instead of as a separate Python process.
pub mod policy;
pub mod source;
pub mod turn_journey;
pub mod turn_location;

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
