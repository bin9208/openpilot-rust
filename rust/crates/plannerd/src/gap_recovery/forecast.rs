use super::{entry_weight, recovery_tau, GapSample, LeadGapState};
use openpilot_control_policy::math::{maximum, minimum};

pub struct GapForecast<'a, const N: usize> {
    pub level: i32,
    pub times: &'a [f64; N],
    pub ego_speeds: &'a [f64; N],
    pub lead_speeds: &'a [f64; N],
    pub base_tf: f64,
    pub lead_distances: Option<&'a [f64; N]>,
    pub desired_distances: Option<&'a [f64; N]>,
}

impl LeadGapState {
    pub fn margins<const N: usize>(&self, input: GapForecast<'_, N>) -> [f64; N] {
        const {
            assert!(N > 0);
        }
        let Some((_, track_id)) = self.key else {
            return [0.; N];
        };
        if recovery_tau(input.level).is_none() {
            return [0.; N];
        }
        let offset = self.relative_speed - (input.lead_speeds[0] - input.ego_speeds[0]);
        let relative: [f64; N] =
            std::array::from_fn(|i| input.lead_speeds[i] - input.ego_speeds[i] + offset);
        let distances: [f64; N] = match input.lead_distances {
            Some(distances) => std::array::from_fn(|i| {
                self.distance + distances[i] - distances[0] + input.times[i] * offset
            }),
            None => {
                let mut accumulated = 0.;
                let mut previous = 0.;
                std::array::from_fn(|i| {
                    accumulated += (input.times[i] - previous) * relative[i];
                    previous = input.times[i];
                    self.distance + accumulated
                })
            }
        };
        let desired: [f64; N] = match input.desired_distances {
            Some(distances) => {
                std::array::from_fn(|i| self.desired_distance + distances[i] - distances[0])
            }
            None => [self.desired_distance; N],
        };
        let mut predicted = self.clone();
        let mut previous_time = 0.;
        std::array::from_fn(|i| {
            let time = input.times[i];
            let dt = time - previous_time;
            if dt > 0. {
                predicted.update(GapSample {
                    level: input.level,
                    track_id,
                    enabled: true,
                    dt,
                    ego_speed: input.ego_speeds[i],
                    lead_speed: input.lead_speeds[i],
                    relative_speed: relative[i],
                    distance: maximum(1e-3, distances[i]),
                    desired_distance: desired[i],
                    base_tf: input.base_tf,
                });
            }
            let margin = maximum(0., input.ego_speeds[i])
                * minimum(predicted.extra_tf, maximum(0., 2.5 - input.base_tf))
                * entry_weight(predicted.strength);
            previous_time = time;
            margin
        })
    }
}
