use super::{trajectory, Input, LongitudinalMpc, MpcSource, PlannerMode, Weights, TIMES};
use crate::{
    lead_response::{self, ResponseInput},
    solver::{Field, Statistic},
    Error,
};
use openpilot_control_policy::math::{interp, maximum};

impl LongitudinalMpc {
    pub fn update(
        &mut self,
        mut input: Input<'_>,
        clock: &mut impl FnMut() -> f64,
    ) -> Result<Option<i32>, Error> {
        let speed = self.initial[1];
        let acceleration = self.initial[2];
        self.status = input.radar.lead_one.status || input.radar.lead_two.status;
        let following = input
            .carrot
            .following_time(input.personality, speed, acceleration)?;
        let (lead0, speed0) = trajectory::lead(&input.radar.lead_one, speed);
        let (lead1, speed1) = trajectory::lead(&input.radar.lead_two, speed);
        let brake = input.carrot.comfort_brake;
        let stop = input.carrot.config.stop_distance;
        self.base_desired_distances = [speed0, speed1].map(|lead_speed| {
            trajectory::desired_distance(speed, lead_speed, following, brake, stop)
        });
        let (mode, cruise, stop_x) = if self.mode == PlannerMode::Blended {
            (self.mode, input.cruise, 1000.)
        } else {
            (
                input.carrot.mode,
                input.carrot.cruise_speed,
                input.carrot.stop_distance,
            )
        };
        let physical = std::array::from_fn(|i| {
            [
                lead0[i][0] + trajectory::stopped_equivalence(lead0[i][1]),
                lead1[i][0] + trajectory::stopped_equivalence(lead1[i][1]),
            ]
        });
        for row in &mut self.parameters {
            row[0] = if input.reset { acceleration } else { -4. };
            row[1] = maximum(
                0.,
                if input.reset {
                    acceleration
                } else {
                    self.maximum_accel
                },
            );
        }
        let obstacles = self.obstacles(&mut input, mode, cruise, stop_x, &physical, following)?;
        let lead_index = usize::from(self.source == MpcSource::Lead1);
        let response_lead = [&input.radar.lead_one, &input.radar.lead_two][lead_index];
        let request = lead_response::request(ResponseInput {
            level: input.carrot.lead_response,
            enabled: mode == PlannerMode::Acc
                && input.response_enabled
                && input.track_frames[lead_index] >= 3,
            source: self.source,
            lead_status: response_lead.status
                && response_lead.radar
                && response_lead.radar_track_id >= 0,
            lead_acceleration: response_lead.a_lead_k,
            ego_acceleration: input.measured_acceleration,
            relative_speed: response_lead.v_rel,
            gap_margin: if response_lead.status {
                response_lead.d_rel - self.base_desired_distances[lead_index]
            } else {
                -1.
            },
            speed_error: cruise - speed,
        });
        let request = self
            .response_state
            .update(request, self.dt, response_lead.radar_track_id)?;
        self.response_active = request.active;
        self.response_level = if request.active { request.level } else { 0 };
        self.set_weights(Weights {
            previous_accel_constraint: input.previous_accel_constraint,
            jerk_factor: input.carrot.following.jerk_factor,
            change_cost_starting: input.change_cost_starting,
            change_cost_factor: request.a_change_cost_factor,
            jerk_cost_factor: request.jerk_cost_factor,
        })?;
        self.gaps(
            &input,
            mode,
            following,
            &[lead0, lead1],
            &physical,
            &obstacles,
        );
        for (index, row) in self.reference.iter_mut().enumerate() {
            row[1] = input.reference.x[index];
            row[2] = input.reference.speed[index];
            row[3] = input.reference.acceleration[index];
            row[5] = input.reference.jerk[index];
        }
        for stage in 0..12 {
            self.solver
                .set(stage, Field::Reference, &self.reference[stage])?;
        }
        self.solver
            .set(12, Field::Reference, &self.reference[12][..5])?;
        for (i, row) in self.parameters.iter_mut().enumerate() {
            row[2] = trajectory::numpy_min(&obstacles.rows[i][..obstacles.columns]);
            row[3] = self.previous_acceleration[i];
            row[4] = following;
            row[6] = brake;
            row[7] = stop;
        }
        self.following_time = following;
        let warning = self.run(clock)?;
        if mode == PlannerMode::Acc {
            self.predicted_danger_margin = 1000.;
            if input.radar.lead_one.status {
                let margins: Vec<_> = TIMES
                    .iter()
                    .enumerate()
                    .filter(|(_, time)| **time > 0.2 && **time < 3.)
                    .map(|(i, _)| {
                        physical[i][0]
                            - self.x[i][0]
                            - self.danger_factor
                                * trajectory::safe_distance(self.x[i][1], following, brake, stop)
                    })
                    .collect();
                self.predicted_danger_margin = trajectory::numpy_min(&margins);
            }
        }
        let crashed = TIMES
            .iter()
            .enumerate()
            .any(|(i, time)| *time < 5. && lead0[i][0] - self.x[i][0] < 0.25);
        self.crash_count = if crashed && input.radar.lead_one.model_prob > 0.9 {
            self.crash_count + 1
        } else {
            0
        };
        if self.mode == PlannerMode::Blended {
            let close = |index| {
                (0..13).any(|i| {
                    (physical[i][index]
                        - trajectory::safe_distance(self.x[i][1], following, brake, stop))
                        - self.x[i][0]
                        < 0.
                })
            };
            if close(0) {
                self.source = MpcSource::Lead0;
            }
            if close(1) && physical[0][1] - physical[0][0] != 0. {
                self.source = MpcSource::Lead1;
            }
        }
        Ok(warning)
    }

    pub fn run(&mut self, clock: &mut impl FnMut() -> f64) -> Result<Option<i32>, Error> {
        for stage in 0..13 {
            self.solver
                .set(stage, Field::Parameters, &self.parameters[stage])?;
        }
        self.solver.set(0, Field::LowerBound, &self.initial)?;
        self.solver.set(0, Field::UpperBound, &self.initial)?;
        self.solution_status = self.solver.solve();
        self.times = [
            Statistic::TotalTime,
            Statistic::QpTime,
            Statistic::LinearizationTime,
            Statistic::IntegratorTime,
        ]
        .map(|field| self.solver.statistic(field));
        for (stage, row) in self.x.iter_mut().enumerate() {
            self.solver.get(stage, Field::State, row)?;
        }
        for (stage, row) in self.u.iter_mut().enumerate() {
            self.solver.get(stage, Field::Control, row)?;
        }
        self.speed = self.x.map(|row| row[1]);
        self.acceleration = self.x.map(|row| row[2]);
        self.jerk = self.u.map(|row| row[0]);
        for (i, value) in self.previous_acceleration.iter_mut().enumerate() {
            *value = interp(TIMES[i] + self.dt, &TIMES, &self.acceleration)?;
        }
        let now = clock();
        let mut warning = None;
        if self.solution_status != 0 {
            if now > self.last_warning + 5. {
                self.last_warning = now;
                warning = Some(self.solution_status);
            }
            self.reset()?;
        }
        Ok(warning)
    }
}
