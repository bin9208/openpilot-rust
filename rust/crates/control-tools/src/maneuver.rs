mod readiness;
pub use readiness::{Lateral, LateralInput, Longitudinal, LongitudinalInput};

use crate::Error;
use openpilot_control_policy::math::interp;
use serde::Deserialize;

pub const DT: f64 = 0.05;

#[derive(Clone, Deserialize)]
pub struct Action {
    pub accel: Vec<f64>,
    pub time: Vec<f64>,
}

#[derive(Clone, Default)]
pub struct State {
    pub active: bool,
    pub finished: bool,
    pub run_completed: bool,
    pub action_index: usize,
    pub action_frames: u32,
    pub ready_count: u64,
    pub repeated: u32,
}

pub struct Sequence {
    actions: Vec<Action>,
    repeat: u32,
    pub initial_speed: f64,
    pub state: State,
}

impl Sequence {
    pub fn new(actions: Vec<Action>, repeat: u32, initial_speed: f64) -> Result<Self, Error> {
        if actions.is_empty()
            || actions
                .iter()
                .any(|action| action.accel.is_empty() || action.accel.len() != action.time.len())
        {
            return Err(Error::Contract(
                "maneuvers require nonempty paired time/acceleration actions",
            ));
        }
        Ok(Self {
            actions,
            repeat,
            initial_speed,
            state: State::default(),
        })
    }

    pub fn reset(&mut self) {
        self.state.active = false;
        self.state.action_frames = 0;
        self.state.action_index = 0;
    }

    pub fn action_remaining(&self) -> Result<f64, Error> {
        let end = self
            .actions
            .get(self.state.action_index)
            .and_then(|action| action.time.last())
            .ok_or(Error::Contract("maneuver action index"))?;
        Ok(end - f64::from(self.state.action_frames) * DT)
    }

    fn step(&mut self, reset_readiness: bool) -> Result<f64, Error> {
        self.state.run_completed = false;
        let action = self
            .actions
            .get(self.state.action_index)
            .ok_or(Error::Contract("maneuver action index"))?;
        let value = interp(
            f64::from(self.state.action_frames) * DT,
            &action.time,
            &action.accel,
        )?;
        let end = action
            .time
            .last()
            .ok_or(Error::Contract("empty maneuver action"))?;
        self.state.action_frames = self
            .state
            .action_frames
            .checked_add(1)
            .ok_or(Error::Contract("maneuver frame overflow"))?;
        if f64::from(self.state.action_frames) > end / DT {
            if self.state.action_index < self.actions.len() - 1 {
                self.state.action_index += 1;
                self.state.action_frames = 0;
            } else if self.state.repeated < self.repeat {
                self.state.repeated += 1;
                self.state.run_completed = true;
                self.reset();
                if reset_readiness {
                    self.state.ready_count = 0;
                }
            } else {
                self.state.run_completed = true;
                self.state.finished = true;
            }
        }
        Ok(value)
    }
}
