use crate::{
    solver::{Acados, Field, Kind},
    Error,
};
use std::{path::Path, time::Instant};

pub struct LateralInput<'a> {
    pub initial: [f64; 4],
    pub parameters: &'a [[f64; 2]; 33],
    pub y: &'a [f64; 33],
    pub heading: &'a [f64; 33],
    pub yaw_rate: &'a [f64; 33],
}

pub struct LateralMpc {
    solver: Acados,
    reference: [[f64; 5]; 33],
    pub x: [[f64; 4]; 33],
    pub u: [[f64; 1]; 32],
    pub status: i32,
    pub solve_time: f64,
    pub cost: f64,
}

impl LateralMpc {
    pub fn load(directory: &Path) -> Result<Self, Error> {
        let mut owner = Self {
            solver: Acados::load(directory, Kind::Lateral)?,
            reference: [[0.; 5]; 33],
            x: [[0.; 4]; 33],
            u: [[0.; 1]; 32],
            status: 0,
            solve_time: 0.,
            cost: 0.,
        };
        owner.reset([0.; 4])?;
        Ok(owner)
    }

    pub fn reset(&mut self, initial: [f64; 4]) -> Result<(), Error> {
        self.x = [[0.; 4]; 33];
        self.u = [[0.; 1]; 32];
        self.reference = [[0.; 5]; 33];
        for stage in 0..32 {
            self.solver
                .set(stage, Field::Reference, &self.reference[stage])?;
        }
        self.solver
            .set(32, Field::Reference, &self.reference[32][..3])?;
        for stage in 0..33 {
            self.solver.set(stage, Field::State, &[0.; 4])?;
            self.solver.set(stage, Field::Parameters, &[0.; 2])?;
        }
        self.solver.set(0, Field::LowerBound, &initial)?;
        self.solver.set(0, Field::UpperBound, &initial)?;
        self.solver.solve();
        self.status = 0;
        self.solve_time = 0.;
        self.cost = 0.;
        Ok(())
    }

    pub fn set_weights(&mut self, weights: [f64; 5]) -> Result<(), Error> {
        let mut stage_weights = [0.; 25];
        for (index, weight) in weights.iter().enumerate() {
            stage_weights[index * 5 + index] = *weight;
        }
        for stage in 0..32 {
            self.solver.set(stage, Field::Weights, &stage_weights)?;
        }
        let mut terminal = [0.; 9];
        for (index, weight) in weights[..3].iter().enumerate() {
            terminal[index * 3 + index] = *weight;
        }
        self.solver.set(32, Field::Weights, &terminal)
    }

    pub fn run(&mut self, input: LateralInput<'_>) -> Result<(), Error> {
        self.solver.set(0, Field::LowerBound, &input.initial)?;
        self.solver.set(0, Field::UpperBound, &input.initial)?;
        let speed_offset = input.parameters[0][0] + 10.;
        for (index, reference) in self.reference.iter_mut().enumerate() {
            reference[0] = input.y[index];
            reference[1] = input.heading[index] * speed_offset;
            reference[2] = input.yaw_rate[index] * speed_offset;
        }
        for stage in 0..32 {
            self.solver
                .set(stage, Field::Reference, &self.reference[stage])?;
            self.solver
                .set(stage, Field::Parameters, &input.parameters[stage])?;
        }
        self.solver
            .set(32, Field::Parameters, &input.parameters[32])?;
        self.solver
            .set(32, Field::Reference, &self.reference[32][..3])?;
        let started = Instant::now();
        self.status = self.solver.solve();
        self.solve_time = started.elapsed().as_secs_f64();
        for (stage, output) in self.x.iter_mut().enumerate() {
            self.solver.get(stage, Field::State, output)?;
        }
        for (stage, output) in self.u.iter_mut().enumerate() {
            self.solver.get(stage, Field::Control, output)?;
        }
        self.cost = self.solver.cost();
        Ok(())
    }
}
