//! Explicit external selection, stopped source switching, and loss acknowledgement.
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum Mode {
    Off,
    ActiveRequest,
}
impl Mode {
    pub fn from_setting(value: i32) -> Self {
        match value {
            // Both historical nonzero settings now request actual external output.
            1 | 2 => Self::ActiveRequest,
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
}
impl Transition {
    pub fn new(previously_active: bool) -> Self {
        Self {
            active: false,
            loss_latched: previously_active,
        }
    }
    pub fn update(
        &mut self,
        mode: Mode,
        ready: bool,
        compatible: bool,
        controls: ControlState,
        outcome: Outcome,
    ) -> Decision {
        let mut reset = false;
        if self.active
            && (matches!(outcome, Outcome::Lost)
                || !ready
                || !compatible
                || mode != Mode::ActiveRequest)
        {
            self.active = false;
            self.loss_latched = true;
        }
        if !self.active && mode == Mode::Off && controls.inactive_stop() {
            self.loss_latched = false;
        }
        if !self.active
            && mode == Mode::ActiveRequest
            && !self.loss_latched
            && ready
            && compatible
            && !matches!(outcome, Outcome::Lost)
            && controls.inactive_stop()
        {
            self.active = true;
            reset = true;
        }
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
