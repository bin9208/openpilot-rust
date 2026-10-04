use super::{trajectory, Input, LongitudinalMpc, MpcSource, PlannerMode, TIMES};
use crate::{
    lane_change_gap::CreditInput,
    lead_obstacles::{cutout_relief, FollowGeometry},
    traffic_stop, Error,
};
use openpilot_control_policy::math::clip;

pub(super) struct Obstacles {
    pub rows: [[f64; 4]; 13],
    pub columns: usize,
}

impl LongitudinalMpc {
    pub(super) fn obstacles(
        &mut self,
        input: &mut Input<'_>,
        mode: PlannerMode,
        cruise: f64,
        stop_x: f64,
        physical: &[[f64; 2]; 13],
        follow: f64,
    ) -> Result<Obstacles, Error> {
        let mut result = Obstacles {
            rows: [[0.; 4]; 13],
            columns: if mode == PlannerMode::Acc { 4 } else { 2 },
        };
        match mode {
            PlannerMode::Acc => {
                let mut integral = 0.;
                let mut previous = 0.;
                let cruise_obstacles: [f64; 13] = std::array::from_fn(|i| {
                    let lower = self.initial[1] + (TIMES[i] * self.cruise_minimum_accel * 1.05);
                    let upper = self.initial[1] + (TIMES[i] * self.maximum_accel * 1.05);
                    let speed = clip(cruise, lower, upper);
                    integral += (TIMES[i] - previous) * speed;
                    previous = TIMES[i];
                    integral
                        + trajectory::safe_distance(
                            speed,
                            follow,
                            input.carrot.comfort_brake,
                            input.carrot.config.stop_distance,
                        )
                });
                let adjustment = traffic_stop::distance_adjust(
                    input.carrot.config.traffic_stop_adjust,
                    self.initial[1],
                    input.carrot.traffic_stop_model_lead_offset,
                );
                let traffic =
                    traffic_stop::obstacle_distance(stop_x, cruise_obstacles[0], adjustment)?;
                let mut relief = [0.; 13];
                if input.cutout_enabled && !input.reset {
                    if input.carrot.lane_change_gap.active {
                        relief.copy_from_slice(&input.carrot.lane_change_gap.credit(
                            Some(&input.radar.lead_one),
                            &TIMES,
                            CreditInput {
                                speed: self.initial[1],
                                max_accel: self.maximum_accel,
                                follow,
                                stop_distance: input.carrot.config.stop_distance,
                                ratio: input.carrot.config.lane_change_ratio,
                            },
                        )?);
                    } else {
                        relief = cutout_relief(
                            &input.radar.lead_one,
                            FollowGeometry {
                                speed: self.initial[1],
                                follow_time: follow,
                                stop_distance: input.carrot.config.stop_distance,
                            },
                            &TIMES,
                        );
                    }
                }
                for i in 0..13 {
                    result.rows[i] = [
                        physical[i][0] + relief[i],
                        physical[i][1],
                        cruise_obstacles[i],
                        traffic,
                    ];
                }
                self.source = [
                    MpcSource::Lead0,
                    MpcSource::Lead1,
                    MpcSource::Cruise,
                    MpcSource::E2e,
                ][trajectory::argmin(&result.rows[0])];
                if cruise == 0. && self.source == MpcSource::Cruise {
                    for row in &mut self.parameters {
                        row[0] = -input.carrot.config.navigation_decel;
                    }
                }
                input.reference.x.fill(0.);
                input.reference.speed.fill(0.);
                input.reference.acceleration.fill(0.);
                input.reference.jerk.fill(0.);
                self.danger_factor = 0.8;
                for row in &mut self.parameters {
                    row[5] = self.danger_factor;
                }
            }
            PlannerMode::Blended => {
                for row in &mut self.parameters {
                    row[5] = 1.;
                }
                let origin = input.reference.x[0];
                let mut position = origin;
                for i in 0..13 {
                    result.rows[i][..2].copy_from_slice(&physical[i]);
                    let target = TIMES[i] * clip(cruise, self.initial[1] - 2., 1e3) + origin;
                    if i > 0 {
                        position += ((input.reference.speed[i] + input.reference.speed[i - 1])
                            / 2.)
                            * (TIMES[i] - TIMES[i - 1]);
                    }
                    if i == 1 {
                        self.source = if position < target {
                            MpcSource::E2e
                        } else {
                            MpcSource::Cruise
                        };
                    }
                    input.reference.x[i] = trajectory::numpy_min(&[position, target]);
                }
            }
        }
        Ok(result)
    }
}
