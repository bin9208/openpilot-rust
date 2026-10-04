use openpilot_control_policy::math::{maximum, minimum};
use serde::Deserialize;

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccelerationSample {
    pub acceleration: f64,
    pub jerk: f64,
    pub time: f64,
    pub measured: bool,
}

#[derive(Clone, Debug)]
pub struct LeadAccelTau {
    tau: f64,
    ordinary_tau: f64,
    previous_strength: f64,
    sample_time: Option<f64>,
}

impl LeadAccelTau {
    pub fn new(initial: f64) -> Self {
        let tau = if initial.is_finite() && (0. ..=1.5).contains(&initial) {
            initial
        } else {
            1.5
        };
        Self {
            tau,
            ordinary_tau: tau,
            previous_strength: 0.,
            sample_time: None,
        }
    }

    pub fn clear_evidence(&mut self) {
        self.previous_strength = 0.;
        self.sample_time = None;
    }

    pub fn update(&mut self, sample: AccelerationSample) -> f64 {
        let AccelerationSample {
            acceleration,
            jerk,
            time,
            measured,
        } = sample;
        if ![acceleration, jerk, time]
            .iter()
            .all(|value| value.is_finite())
        {
            self.clear_evidence();
            self.tau = 1.5;
            self.ordinary_tau = self.tau;
            return self.tau;
        }
        let mut strength = if measured {
            maximum(0., minimum(1., (-acceleration - 0.5) / (1.5 - 0.5)))
                * maximum(0., minimum(1., (-jerk - 1.5) / (3. - 1.5)))
        } else {
            0.
        };
        let mut confirmed = 0.;
        if let Some(previous) = self.sample_time {
            if time < previous {
                strength = 0.;
            }
            if time - previous > 0. && time - previous <= 0.15 {
                confirmed = minimum(strength, self.previous_strength);
            }
        }
        if self.sample_time != Some(time) {
            self.previous_strength = strength;
            self.sample_time = Some(time);
        }
        if !measured {
            self.previous_strength = 0.;
        }
        if acceleration.abs() < 0.5 && jerk.abs() < 0.5 {
            self.tau = 1.5;
            self.ordinary_tau = self.tau;
        } else {
            let ordinary_alpha = 0.05 / (0.45 + 0.05);
            self.ordinary_tau *= 1. - ordinary_alpha;
            let fast_alpha = 0.05 / (0.05 + 0.05);
            let alpha = ordinary_alpha + confirmed * (fast_alpha - ordinary_alpha);
            self.tau *= 1. - alpha;
            if acceleration >= 0. {
                self.tau = self.ordinary_tau;
            }
        }
        self.tau
    }
}
