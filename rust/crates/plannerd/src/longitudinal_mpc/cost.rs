use super::{LongitudinalMpc, PlannerMode, TIMES};
use crate::{solver::Field, Error};
use openpilot_control_policy::math::{clip, interp};

#[derive(Clone, Copy, Debug)]
pub struct Weights {
    pub previous_accel_constraint: bool,
    pub jerk_factor: f64,
    pub change_cost_starting: f64,
    pub change_cost_factor: f64,
    pub jerk_cost_factor: f64,
}

impl Default for Weights {
    fn default() -> Self {
        Self {
            previous_accel_constraint: true,
            jerk_factor: 1.,
            change_cost_starting: 10.,
            change_cost_factor: 1.,
            jerk_cost_factor: 1.,
        }
    }
}

impl LongitudinalMpc {
    pub fn set_weights(&mut self, input: Weights) -> Result<(), Error> {
        let costs = match self.mode {
            PlannerMode::Acc => {
                self.change_cost = (if input.previous_accel_constraint {
                    200.
                } else {
                    input.change_cost_starting
                }) * clip(input.change_cost_factor, 0., 1.);
                self.jerk_cost_factor = clip(input.jerk_cost_factor, 0., 1.);
                [
                    5.,
                    0.,
                    0.,
                    0.,
                    self.change_cost,
                    input.jerk_factor * self.jerk_cost_factor * 20.,
                ]
            }
            PlannerMode::Blended => {
                self.change_cost = if input.previous_accel_constraint {
                    40.
                } else {
                    0.
                };
                self.jerk_cost_factor = 1.;
                [0., 0.1, 0.2, 5., self.change_cost, 1.]
            }
        };
        let values = [
            costs[0], costs[1], costs[2], costs[3], costs[4], costs[5], 1e6, 1e6, 1e6, 100.,
        ];
        let key = values
            .iter()
            .all(|v| v.is_finite())
            .then(|| (self.mode, values.map(f64::to_bits)));
        // The private solver is created once per owner and never replaced; reset invalidates this key.
        if key.is_some() && key == self.cost_key {
            return Ok(());
        }
        self.cost_key = None;
        let mut weights = [0.; 36];
        for i in 0..6 {
            weights[i * 6 + i] = costs[i];
        }
        for (stage, time) in TIMES[..12].iter().enumerate() {
            weights[28] = costs[4] * interp(*time, &[0., 1., 2.], &[1., 1., 0.])?;
            self.solver.set(stage, Field::Weights, &weights)?;
        }
        let mut terminal = [0.; 25];
        for row in 0..5 {
            for column in 0..5 {
                terminal[row * 5 + column] = weights[row * 6 + column];
            }
        }
        self.solver.set(12, Field::Weights, &terminal)?;
        for stage in 0..12 {
            self.solver
                .set(stage, Field::LowerSlack, &[1e6, 1e6, 1e6, 100.])?;
        }
        self.cost_key = key;
        Ok(())
    }
}
