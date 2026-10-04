use crate::{
    carrot::CarrotPlanner,
    gap_recovery::LeadGapState,
    lead_response::LeadResponseState,
    radar::Radar,
    solver::{Acados, Field, Kind},
    types::{MpcSource, Personality, PlannerMode},
    Error,
};
use std::path::Path;

mod cost;
mod gaps;
mod obstacles;
mod trajectory;
mod update;
pub use cost::Weights;
pub use trajectory::{desired_distance, safe_distance, stopped_equivalence, TIMES};

#[derive(Clone, Debug, serde::Deserialize)]
pub struct Reference {
    pub x: [f64; 13],
    pub speed: [f64; 13],
    pub acceleration: [f64; 13],
    pub jerk: [f64; 13],
}

pub struct Input<'a> {
    pub carrot: &'a mut CarrotPlanner,
    pub radar: &'a Radar,
    pub reset: bool,
    pub cruise: f64,
    pub reference: Reference,
    pub personality: Personality,
    pub previous_accel_constraint: bool,
    pub change_cost_starting: f64,
    pub response_enabled: bool,
    pub gap_enabled: bool,
    pub track_frames: [u64; 2],
    pub measured_acceleration: f64,
    pub cutout_enabled: bool,
}

pub struct LongitudinalMpc {
    solver: Acados,
    pub mode: PlannerMode,
    dt: f64,
    pub source: MpcSource,
    pub x: [[f64; 3]; 13],
    pub u: [[f64; 1]; 12],
    pub parameters: [[f64; 8]; 13],
    pub reference: [[f64; 6]; 13],
    pub initial: [f64; 3],
    pub speed: [f64; 13],
    pub acceleration: [f64; 13],
    pub jerk: [f64; 12],
    pub previous_acceleration: [f64; 13],
    pub following_time: f64,
    pub desired_distance: f64,
    pub base_desired_distances: [f64; 2],
    pub danger_factor: f64,
    pub predicted_danger_margin: f64,
    pub change_cost: f64,
    pub jerk_cost_factor: f64,
    pub response_active: bool,
    pub response_level: i32,
    response_state: LeadResponseState,
    gap_states: [LeadGapState; 2],
    pub gap_margins: [[f64; 2]; 13],
    pub status: bool,
    pub crash_count: u64,
    pub solution_status: i32,
    pub times: [f64; 4],
    last_warning: f64,
    pub cruise_minimum_accel: f64,
    pub maximum_accel: f64,
}

impl LongitudinalMpc {
    pub fn load(directory: &Path, mode: PlannerMode, dt: f64) -> Result<Self, Error> {
        let mut owner = Self {
            solver: Acados::load(directory, Kind::Longitudinal)?,
            mode,
            dt,
            source: MpcSource::Cruise,
            x: [[0.; 3]; 13],
            u: [[0.; 1]; 12],
            parameters: [[0.; 8]; 13],
            reference: [[0.; 6]; 13],
            initial: [0.; 3],
            speed: [0.; 13],
            acceleration: [0.; 13],
            jerk: [0.; 12],
            previous_acceleration: [0.; 13],
            following_time: 1.,
            desired_distance: 0.,
            base_desired_distances: [0.; 2],
            danger_factor: 0.8,
            predicted_danger_margin: 1000.,
            change_cost: 200.,
            jerk_cost_factor: 1.,
            response_active: false,
            response_level: 0,
            response_state: LeadResponseState::default(),
            gap_states: std::array::from_fn(|_| LeadGapState::default()),
            gap_margins: [[0.; 2]; 13],
            status: false,
            crash_count: 0,
            solution_status: 0,
            times: [0.; 4],
            last_warning: 0.,
            cruise_minimum_accel: 0.,
            maximum_accel: 0.,
        };
        owner.reset()?;
        Ok(owner)
    }

    pub fn reset(&mut self) -> Result<(), Error> {
        self.solver.reset()?;
        self.speed = [0.; 13];
        self.acceleration = [0.; 13];
        self.previous_acceleration = [0.; 13];
        self.jerk = [0.; 12];
        self.reference = [[0.; 6]; 13];
        for stage in 0..12 {
            self.solver
                .set(stage, Field::Reference, &self.reference[stage])?;
        }
        self.solver
            .set(12, Field::Reference, &self.reference[12][..5])?;
        self.x = [[0.; 3]; 13];
        self.u = [[0.; 1]; 12];
        self.parameters = [[0.; 8]; 13];
        for stage in 0..13 {
            self.solver.set(stage, Field::State, &[0.; 3])?;
        }
        self.last_warning = 0.;
        self.status = false;
        self.crash_count = 0;
        self.predicted_danger_margin = 1000.;
        self.solution_status = 0;
        self.response_active = false;
        self.response_level = 0;
        self.response_state = LeadResponseState::default();
        self.gap_states = std::array::from_fn(|_| LeadGapState::default());
        self.gap_margins = [[0.; 2]; 13];
        self.times = [0.; 4];
        self.initial = [0.; 3];
        self.set_weights(Weights::default())
    }

    pub fn current_state(&mut self, speed: f64, acceleration: f64) -> Result<(), Error> {
        let previous = self.initial[1];
        self.initial[1] = speed;
        self.initial[2] = acceleration;
        if (previous - speed).abs() > 2. {
            for stage in 0..13 {
                self.solver.set(stage, Field::State, &self.initial)?;
            }
        }
        Ok(())
    }

    pub fn acceleration_limits(&mut self, minimum: f64, maximum: f64) {
        self.cruise_minimum_accel = minimum;
        self.maximum_accel = maximum;
    }
}
