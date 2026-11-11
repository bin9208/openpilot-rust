//! Lamp-only physical-indicator observers of carrot's `desire_lib`. They
//! decide when to light a lamp for a maneuver the driver or model is
//! already making; none of them produces a lane-change desire or steering.
pub mod boundary;
pub mod echo;
pub mod lane;
pub mod manual_turn;
pub mod observer;
#[cfg(test)]
mod tests;
