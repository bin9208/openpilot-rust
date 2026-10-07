use openpilot_control_policy::math::{maximum, minimum};
use serde::Deserialize;

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoastingInput {
    pub enabled: bool,
    pub percent: f64,
    pub set_speed: f64,
    pub target: f64,
    pub external_limit: f64,
    pub dt: f64,
}

#[derive(Debug, Default)]
pub struct CruiseCoastingPlan {
    target: f64,
    set_speed: f64,
    percent: f64,
    stable_time: f64,
}

impl CruiseCoastingPlan {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn update(&mut self, input: &CoastingInput) -> f64 {
        let percent = if input.percent.is_finite() {
            minimum(10., maximum(0., input.percent)).trunc()
        } else {
            0.
        };
        if !input.enabled
            || percent == 0.
            || ![
                input.set_speed,
                input.target,
                input.external_limit,
                input.dt,
            ]
            .iter()
            .all(|value| value.is_finite())
            || input.target <= 10. / 3.6
            || input.dt <= 0.
            || input.external_limit <= input.target * (1. + percent / 100.)
        {
            *self = Self::default();
            return 0.;
        }
        if percent != self.percent
            || (input.set_speed - self.set_speed).abs() > 0.001
            || (input.target - self.target).abs() > 0.02
        {
            self.target = input.target;
            self.set_speed = input.set_speed;
            self.percent = percent;
            self.stable_time = 0.;
        } else {
            self.stable_time = minimum(1., self.stable_time + input.dt);
        }
        if self.stable_time >= 1. {
            self.target
        } else {
            0.
        }
    }

    pub const fn stable_time(&self) -> f64 {
        self.stable_time
    }
}
