//! Carrot modeld/jetlink/model.py JoiningModel switch: join a ready Jetlink
//! session at a fresh vehicle-state gate, return to the local model on any
//! failure and rejoin later. No setting, validation record or loss latch.
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum Outcome {
    None,
    Valid,
    Lost,
}
/// `join_allowed` is model_join.may_join: carState, selfdriveState and
/// carControl valid and received within 250 ms, and either stopped
/// (standstill, |vEgo| < 0.01) or the driver holding the wheel.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct ControlState {
    pub join_allowed: bool,
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
#[derive(Default)]
pub struct Transition {
    active: bool,
}
impl Transition {
    pub fn update(&mut self, ready: bool, controls: ControlState, outcome: Outcome) -> Decision {
        let mut reset = false;
        let lost = matches!(outcome, Outcome::Lost);
        if self.active && (lost || !ready) {
            self.active = false;
        }
        if !self.active && !lost && ready && controls.join_allowed {
            self.active = true;
            reset = true;
        }
        Decision {
            source: if self.active {
                Source::Jetlink
            } else {
                Source::Native
            },
            phase: if self.active {
                Phase::Active
            } else if lost {
                Phase::Lost
            } else if ready {
                Phase::Ready
            } else {
                Phase::Preparing
            },
            loss_latched: false,
            reset_required: reset,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn joins_at_the_gate_and_rejoins_after_loss() {
        let mut t = Transition::default();
        let gate = |join_allowed| ControlState { join_allowed };
        assert_eq!(
            t.update(true, gate(false), Outcome::None).source,
            Source::Native
        );
        let joined = t.update(true, gate(true), Outcome::None);
        assert_eq!(joined.source, Source::Jetlink);
        assert!(joined.reset_required);
        // Once active the gate is not re-checked while driving.
        assert_eq!(
            t.update(true, gate(false), Outcome::None).source,
            Source::Jetlink
        );
        assert_eq!(
            t.update(false, gate(true), Outcome::Lost).phase,
            Phase::Lost
        );
        assert!(!t.update(true, gate(true), Outcome::Lost).loss_latched);
        assert_eq!(
            t.update(true, gate(true), Outcome::None).source,
            Source::Jetlink
        );
    }
}
