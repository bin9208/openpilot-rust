use super::Sequence;
use crate::Error;
use openpilot_control_policy::math::{maximum, minimum};

#[derive(Clone, Copy)]
pub struct LongitudinalInput {
    pub speed: f64,
    pub active: bool,
    pub standstill: bool,
    pub cruise_standstill: bool,
}

pub struct Longitudinal {
    pub sequence: Sequence,
}

impl Longitudinal {
    pub const fn new(sequence: Sequence) -> Self {
        Self { sequence }
    }

    pub fn update(&mut self, input: LongitudinalInput) -> Result<f64, Error> {
        let owner = &mut self.sequence;
        let mut ready = (input.speed - owner.initial_speed).abs() < 0.3
            && input.active
            && !input.cruise_standstill;
        if owner.initial_speed < 0.01 {
            ready = ready && input.standstill;
        }
        owner.state.ready_count = if ready {
            owner
                .state
                .ready_count
                .checked_add(1)
                .ok_or(Error::Contract("maneuver readiness overflow"))?
        } else {
            0
        };
        if owner.state.ready_count > 60 {
            owner.state.active = true;
        }
        if !owner.state.active {
            return Ok(minimum(
                maximum(owner.initial_speed - input.speed, -2.0),
                2.0,
            ));
        }
        owner.step(false)
    }
}

#[derive(Clone, Copy)]
pub struct LateralInput {
    pub speed: f64,
    pub active: bool,
    pub curvature: f64,
    pub roll: f64,
}

pub struct Lateral {
    pub sequence: Sequence,
    pub baseline_curvature: f64,
}

impl Lateral {
    pub const fn new(sequence: Sequence) -> Self {
        Self {
            sequence,
            baseline_curvature: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.sequence.reset();
        self.sequence.state.ready_count = 0;
    }

    pub fn update(&mut self, input: LateralInput) -> Result<f64, Error> {
        let owner = &mut self.sequence;
        owner.state.run_completed = false;
        let ready = (input.speed - owner.initial_speed).abs() < 0.7
            && input.active
            && input.curvature.abs() < 0.002
            && input.roll.abs() < 0.08;
        owner.state.ready_count = if ready {
            owner
                .state
                .ready_count
                .checked_add(1)
                .ok_or(Error::Contract("maneuver readiness overflow"))?
        } else {
            owner.state.ready_count.saturating_sub(1)
        };
        if owner.state.ready_count > 40 {
            if !owner.state.active {
                self.baseline_curvature = input.curvature;
            }
            owner.state.active = true;
        }
        if !owner.state.active {
            return Ok(0.0);
        }
        owner.step(true)
    }
}
