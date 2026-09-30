//! Source-compatible stopped acknowledgement and loss latch.
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum Mode {
    Off,
    Shadow,
    ActiveRequest,
}
impl Mode {
    pub fn from_setting(value: i32) -> Self {
        match value {
            1 => Self::Shadow,
            2 => Self::ActiveRequest,
            _ => Self::Off,
        }
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum Outcome {
    None,
    Valid,
    Lost,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct ControlState {
    pub standstill: bool,
    pub cruise_enabled: bool,
    pub lateral_active: bool,
    pub enabled: bool,
}
impl ControlState {
    pub fn inactive_stop(self) -> bool {
        self.standstill && !(self.cruise_enabled || self.lateral_active || self.enabled)
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Native,
    Jetlink,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Phase {
    Off,
    Preparing,
    Ready,
    Active,
    Lost,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Decision {
    pub source: Source,
    pub phase: Phase,
    pub loss_latched: bool,
    pub reset_required: bool,
}
pub struct Transition {
    active: bool,
    loss_latched: bool,
    armed: bool,
    previous_mode: Mode,
}
impl Transition {
    pub fn new(previously_active: bool) -> Self {
        Self {
            active: false,
            loss_latched: previously_active,
            armed: false,
            previous_mode: Mode::ActiveRequest,
        }
    }
    pub fn update(
        &mut self,
        mode: Mode,
        ready: bool,
        validated: bool,
        controls: ControlState,
        outcome: Outcome,
    ) -> Decision {
        let mut reset = false;
        if self.active
            && (matches!(outcome, Outcome::Lost)
                || !ready
                || !validated
                || mode != Mode::ActiveRequest)
        {
            self.active = false;
            self.loss_latched = true;
            self.armed = false;
        }
        if !self.active && mode != Mode::ActiveRequest && controls.inactive_stop() {
            if ready || mode == Mode::Off {
                self.loss_latched = false;
            }
            self.armed = true;
        }
        let edge = mode == Mode::ActiveRequest && self.previous_mode != Mode::ActiveRequest;
        if !self.active
            && edge
            && self.armed
            && !self.loss_latched
            && ready
            && validated
            && controls.inactive_stop()
        {
            self.active = true;
            reset = true;
            self.armed = false;
        }
        self.previous_mode = mode;
        Decision {
            source: if self.active {
                Source::Jetlink
            } else {
                Source::Native
            },
            phase: if self.loss_latched {
                Phase::Lost
            } else if self.active {
                Phase::Active
            } else if mode == Mode::Off {
                Phase::Off
            } else if ready {
                Phase::Ready
            } else {
                Phase::Preparing
            },
            loss_latched: self.loss_latched,
            reset_required: reset,
        }
    }
}
