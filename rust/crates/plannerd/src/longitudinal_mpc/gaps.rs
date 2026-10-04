use super::{obstacles::Obstacles, trajectory, Input, LongitudinalMpc, PlannerMode, TIMES};
use crate::gap_recovery::{GapForecast, GapSample};
use openpilot_control_policy::math::maximum;

impl LongitudinalMpc {
    pub(super) fn gaps(
        &mut self,
        input: &Input<'_>,
        mode: PlannerMode,
        following: f64,
        trajectories: &[[[f64; 2]; 13]; 2],
        physical: &[[f64; 2]; 13],
        obstacles: &Obstacles,
    ) {
        let speed = self.initial[1];
        let predicted_speed = self
            .x
            .map(|row| trajectory::numpy_max_zero(row[1] + speed - self.x[0][1]));
        let mut integral = 0.;
        let mut previous_time = 0.;
        let predicted_distance: [f64; 13] = std::array::from_fn(|i| {
            let velocity = if i == 0 {
                predicted_speed[0]
            } else {
                (predicted_speed[i] + predicted_speed[i - 1]) * 0.5
            };
            integral += (TIMES[i] - previous_time) * velocity;
            previous_time = TIMES[i];
            integral
        });
        self.gap_margins.fill([0.; 2]);
        for (index, lead) in [&input.radar.lead_one, &input.radar.lead_two]
            .into_iter()
            .enumerate()
        {
            let eligible = mode == PlannerMode::Acc
                && input.gap_enabled
                && !input.reset
                && !input.carrot.lane_change_active
                && input.track_frames[index] >= 3
                && lead.status
                && lead.radar;
            self.gap_states[index].update(GapSample {
                level: input.carrot.lead_response,
                track_id: lead.radar_track_id,
                enabled: eligible,
                dt: self.dt,
                ego_speed: speed,
                lead_speed: if eligible { lead.v_lead } else { 0. },
                relative_speed: if eligible { lead.v_rel } else { 0. },
                distance: if eligible { lead.d_rel } else { 0. },
                desired_distance: self.base_desired_distances[index],
                base_tf: following,
            });
            let lead_speeds = trajectories[index].map(|row| row[1]);
            let distances =
                std::array::from_fn(|i| trajectories[index][i][0] - predicted_distance[i]);
            let desired = std::array::from_fn(|i| {
                trajectory::desired_distance(
                    predicted_speed[i],
                    lead_speeds[i],
                    following,
                    input.carrot.comfort_brake,
                    input.carrot.config.stop_distance,
                )
            });
            let margins = self.gap_states[index].margins(GapForecast {
                level: input.carrot.lead_response,
                times: &TIMES,
                ego_speeds: &predicted_speed,
                lead_speeds: &lead_speeds,
                base_tf: following,
                lead_distances: Some(&distances),
                desired_distances: Some(&desired),
            });
            for (row, margin) in self.gap_margins.iter_mut().zip(margins) {
                row[index] = margin;
            }
        }
        let preferred: [f64; 2] =
            std::array::from_fn(|index| obstacles.rows[0][index] - self.gap_margins[0][index]);
        let statuses = [input.radar.lead_one.status, input.radar.lead_two.status];
        let valid: [bool; 2] = std::array::from_fn(|i| {
            statuses[i]
                && preferred[i].is_finite()
                && self.base_desired_distances[i].is_finite()
                && physical[0][i].is_finite()
        });
        self.desired_distance = if valid.iter().any(|v| *v) {
            let selected = trajectory::argmin(&std::array::from_fn::<_, 2, _>(|i| {
                if valid[i] {
                    preferred[i]
                } else {
                    f64::INFINITY
                }
            }));
            let relief = physical[0][selected] - obstacles.rows[0][selected];
            maximum(
                0.,
                self.base_desired_distances[selected] + self.gap_margins[0][selected] + relief,
            )
        } else {
            0.
        };
        for (i, speed) in predicted_speed.iter().enumerate() {
            let mut preferred = obstacles.rows[i];
            for (column, value) in preferred[..2].iter_mut().enumerate() {
                *value -= self.gap_margins[i][column];
            }
            let shift = trajectory::numpy_max_zero(
                trajectory::numpy_min(&obstacles.rows[i][..obstacles.columns])
                    - trajectory::numpy_min(&preferred[..obstacles.columns]),
            );
            self.reference[i][0] = shift / (trajectory::numpy_max_zero(*speed) + 10.);
        }
    }
}
