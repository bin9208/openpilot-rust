use openpilot_control_policy::math::{maximum, minimum};
use serde::{Deserialize, Serialize};

mod forecast;
mod headroom;
pub use forecast::GapForecast;

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GapSample {
    pub level: i32,
    pub track_id: i32,
    pub enabled: bool,
    pub dt: f64,
    pub ego_speed: f64,
    pub lead_speed: f64,
    pub relative_speed: f64,
    pub distance: f64,
    pub desired_distance: f64,
    pub base_tf: f64,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct LeadGapState {
    key: Option<(i32, i32)>,
    extra_tf: f64,
    recovery_tf: f64,
    strength: f64,
    filtered_relative_speed: f64,
    candidate: f64,
    distance: f64,
    desired_distance: f64,
    relative_speed: f64,
}

pub(super) fn recovery_tau(level: i32) -> Option<f64> {
    match level {
        0 => Some(5.),
        1 => Some(4.),
        2 => Some(3.),
        3 => Some(2.),
        4 => Some(1.),
        _ => None,
    }
}

pub fn entry_weight(strength: f64) -> f64 {
    strength * strength * (3. - 2. * strength)
}

impl LeadGapState {
    pub fn update(&mut self, input: GapSample) -> f64 {
        let Some(tau) = recovery_tau(input.level) else {
            *self = Self::default();
            return 0.;
        };
        if !input.enabled
            || input.track_id < 0
            || ![
                input.dt,
                input.ego_speed,
                input.lead_speed,
                input.relative_speed,
                input.distance,
                input.desired_distance,
                input.base_tf,
            ]
            .iter()
            .all(|value| value.is_finite())
            || input.dt <= 0.
            || input.ego_speed < 0.
            || input.distance <= 0.
            || input.base_tf < 0.
        {
            *self = Self::default();
            return 0.;
        }
        let cap = maximum(0., 2.5 - input.base_tf);
        let candidate = minimum(
            cap,
            0.5 * maximum(0., input.distance - input.desired_distance)
                / maximum(input.ego_speed, 1.),
        );
        let key = (input.level, input.track_id);
        let acquired = self.key != Some(key);
        if acquired {
            self.key = Some(key);
            self.extra_tf = candidate;
            self.strength = 0.;
            self.recovery_tf = candidate;
            self.filtered_relative_speed = input.relative_speed;
        }
        self.filtered_relative_speed +=
            -(-input.dt / 0.3).exp_m1() * (input.relative_speed - self.filtered_relative_speed);
        if !acquired {
            let target = if self.filtered_relative_speed > 0.2 {
                Some(maximum(maximum(candidate, self.recovery_tf), self.extra_tf))
            } else {
                None
            };
            let result = headroom::advance(
                headroom::Headroom {
                    reservoir: self.recovery_tf,
                    extra: self.extra_tf,
                },
                target,
                headroom::Recovery {
                    dt: input.dt,
                    lead_speed: input.lead_speed,
                    tau,
                    cap,
                },
            );
            self.recovery_tf = result.reservoir;
            self.extra_tf = result.extra;
        }
        self.extra_tf = minimum(cap, self.extra_tf);
        self.recovery_tf = minimum(cap, self.recovery_tf);
        self.strength = minimum(1., self.strength + input.dt / 0.8);
        self.candidate = candidate;
        self.distance = input.distance;
        self.desired_distance = input.desired_distance;
        self.relative_speed = input.relative_speed;
        self.extra_tf * entry_weight(self.strength)
    }
}
