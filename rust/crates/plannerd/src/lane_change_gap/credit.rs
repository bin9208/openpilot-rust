use super::{check_time, GapLead, Plan};
use crate::{lead::Lead, Error};
use openpilot_control_policy::math::{clip, maximum, minimum};

#[derive(Clone, Copy, Debug, serde::Deserialize)]
pub struct CreditInput {
    pub speed: f64,
    pub max_accel: f64,
    pub follow: f64,
    pub stop_distance: f64,
    pub ratio: f64,
}

impl Plan {
    pub fn credit(
        &self,
        primary: Option<&Lead>,
        horizons: &[f64],
        input: CreditInput,
    ) -> Result<Vec<f64>, Error> {
        let empty = || vec![0.; horizons.len()];
        let Some(lead) = GapLead::read(primary) else {
            return Ok(empty());
        };
        let CreditInput {
            speed,
            max_accel,
            follow,
            stop_distance,
            ratio,
        } = input;
        let enabled = self.active
            && 0. < self.confidence
            && self.confidence <= 1.
            && 0. < self.clearance
            && self.clearance <= 2.5
            && !self.targets.is_empty();
        if !enabled
            || lead.id != self.primary_id
            || lead.speed < 4.
            || lead.acceleration < -1.
            || ![speed, max_accel, follow, stop_distance, ratio]
                .iter()
                .all(|v| v.is_finite())
            || !(5. ..=35.).contains(&speed)
            || follow < 0.8
            || !(0. < ratio && ratio < 1.)
        {
            return Ok(empty());
        }
        let accel = maximum(0., max_accel);
        for (index, obstacle) in std::iter::once(&lead)
            .chain(self.targets.iter())
            .enumerate()
        {
            let brake = minimum(-1., obstacle.acceleration);
            for sample in 0..61 {
                let time = check_time(sample);
                let ego_x = speed * time + 0.5 * accel * time.powi(2);
                let ego_speed = speed + accel * time;
                let moving = minimum(time, maximum(0., obstacle.speed) / -brake);
                let obstacle_x =
                    obstacle.distance + obstacle.speed * moving + 0.5 * brake * moving.powi(2);
                let obstacle_speed = maximum(0., obstacle.speed + brake * time);
                let gap = obstacle_x - ego_x;
                let blocked = if index == 0 {
                    let minimum = maximum(8., stop_distance + 2.)
                        + maximum(0., ego_speed - obstacle_speed).powi(2) / 5.;
                    time <= self.clearance + 0.35 && gap <= minimum
                } else {
                    let minimum = stop_distance
                        + follow * ego_speed
                        + maximum(0., ego_speed.powi(2) - obstacle_speed.powi(2)) / 5.;
                    gap <= minimum
                };
                if blocked {
                    return Ok(empty());
                }
            }
        }
        let amount = minimum(
            minimum(4., 0.25 * speed),
            (1. - maximum(0.8, ratio)) * follow * speed,
        ) * self.confidence;
        horizons
            .iter()
            .map(|time| Ok(amount * clip((time - self.clearance - 0.35) / 0.5, 0., 1.)))
            .collect()
    }
}
