use crate::config::{TURN_SIGNAL_BLINK_ON_SECONDS, TURN_SIGNAL_BLINK_PERIOD_SECONDS};
use crate::models::ClusterUiState;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Left,
    Right,
}

#[derive(Default)]
pub struct SignalState {
    left_started: Option<f64>,
    right_started: Option<f64>,
    hazard_started: Option<f64>,
    highlighted: Option<Side>,
    highlight_started: Option<f64>,
}

impl SignalState {
    pub fn lights(&mut self, state: &ClusterUiState, now: f64) -> (bool, bool) {
        if state.left_signal && state.right_signal {
            if self.hazard_started.is_none() {
                self.hazard_started = Some(now);
                self.left_started = None;
                self.right_started = None;
            }
            let lit = blink(now, self.hazard_started.unwrap_or(now));
            return (lit, lit);
        }
        self.hazard_started = None;
        (
            turn(&mut self.left_started, state.left_signal, now),
            turn(&mut self.right_started, state.right_signal, now),
        )
    }

    pub fn highlight(&mut self, state: &ClusterUiState, lights: (bool, bool), now: f64) -> bool {
        let side = match state.highlight_lane.as_deref() {
            Some("left") => Some(Side::Left),
            Some("right") => Some(Side::Right),
            _ => None,
        };
        if let Some(side) = side.filter(|_| state.highlight_lane_offset.is_some()) {
            if self.highlighted != Some(side) || self.highlight_started.is_none() {
                self.highlighted = Some(side);
                self.highlight_started = Some(now);
            }
            let (active, lit) = if side == Side::Left {
                (state.left_signal, lights.0)
            } else {
                (state.right_signal, lights.1)
            };
            let started = if state.left_signal && state.right_signal {
                self.hazard_started
            } else if side == Side::Left {
                self.left_started
            } else {
                self.right_started
            };
            if let Some(started) = started {
                self.highlight_started = Some(started);
            }
            if active {
                return lit;
            }
            return blink(now, self.highlight_started.unwrap_or(now));
        }
        self.highlighted = None;
        self.highlight_started = None;
        if state.left_signal == state.right_signal {
            true
        } else if state.left_signal {
            lights.0
        } else {
            lights.1
        }
    }
}

fn turn(started: &mut Option<f64>, active: bool, now: f64) -> bool {
    if !active {
        *started = None;
        return false;
    }
    blink(now, *started.get_or_insert(now))
}
fn blink(now: f64, started: f64) -> bool {
    (now - started).rem_euclid(TURN_SIGNAL_BLINK_PERIOD_SECONDS) < TURN_SIGNAL_BLINK_ON_SECONDS
}
