//! Source-ordered white/colored speed filters and velocity-aided range.
use super::{config::NativeInterfaceConfig, objects::NativeObject};
use crate::{
    scalar::{divide, square},
    Error,
};
use std::collections::BTreeMap;
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Fusion {
    pub config: NativeInterfaceConfig,
    #[serde(with = "super::float_maps")]
    pub fused: BTreeMap<u64, Vec<f64>>,
    #[serde(with = "super::float_maps")]
    pub range_est: BTreeMap<u64, Vec<f64>>,
}
fn root(value: f64) -> f64 {
    if value == 0. {
        0.
    } else {
        value.sqrt()
    }
}
impl Fusion {
    pub fn new(config: NativeInterfaceConfig) -> Self {
        Self {
            config,
            fused: BTreeMap::new(),
            range_est: BTreeMap::new(),
        }
    }
    pub fn speed(
        &mut self,
        id: u64,
        time: f64,
        object: &NativeObject,
        extra: &[(f64, f64)],
        ego: f64,
    ) -> Result<(f64, f64), Error> {
        let c = &self.config;
        let mut young = 1.;
        if object.age < c.young_age {
            let w = ((c.young_age - object.age) as f64 / (c.young_age - 60).max(1) as f64)
                .clamp(0., 1.);
            young = 1. + (c.young_sigma_scale - 1.) * w;
        }
        if c.speed_bias_tau_s > 0. {
            let scale = object.vel_unc_code.max(1) as f64
                * young
                * (1. + c.speed_sigma_ego_gain * ego / 30.)
                * (1. + c.speed_sigma_accel_gain * object.accel_like_code.abs() as f64 / 100.);
            return self.colored(id, time, object, extra, scale);
        }
        let mut sig = c.speed_sigma_per_code * object.vel_unc_code.max(1) as f64;
        if object.age < c.young_age {
            sig *= young;
        }
        let ground = object.v_long_ground * c.vground_scale;
        let previous = self
            .fused
            .get(&id)
            .filter(|state| state.len() == 3 && 0. < time - state[0] && time - state[0] <= 0.5);
        let Some(previous) = previous else {
            self.fused.insert(id, vec![time, ground, sig * sig]);
            return Ok((ground, sig));
        };
        let (old, mut speed, mut variance) = (previous[0], previous[1], previous[2]);
        variance += square(c.lead_accel_std_mps2 * (time - old))?;
        for (measurement, noise) in std::iter::once((ground, sig * sig))
            .chain(extra.iter().map(|(value, sigma)| (*value, sigma * sigma)))
        {
            let total = variance + noise;
            let mut innovation = measurement - speed;
            let limit = c.innovation_gate_sigma * root(total);
            if c.innovation_gate_sigma > 0. && innovation.abs() > limit {
                innovation = if innovation > 0. { limit } else { -limit };
            }
            let gain = divide(variance, total)?;
            speed += gain * innovation;
            variance *= 1. - gain;
        }
        self.fused.insert(id, vec![time, speed, variance]);
        Ok((speed, root(variance)))
    }
    fn colored(
        &mut self,
        id: u64,
        time: f64,
        object: &NativeObject,
        extra: &[(f64, f64)],
        scale: f64,
    ) -> Result<(f64, f64), Error> {
        let c = &self.config;
        let white = c.speed_sigma_per_code * scale;
        let bias_sigma = c.speed_bias_sigma_per_code * scale;
        let ground = object.v_long_ground * c.vground_scale;
        let previous = self
            .fused
            .get(&id)
            .filter(|state| state.len() == 6 && 0. < time - state[0] && time - state[0] <= 0.5);
        let Some(previous) = previous else {
            let p00 = white * white
                + bias_sigma * bias_sigma
                + square(c.speed_init_sigma_per_code * scale)?;
            self.fused
                .insert(id, vec![time, ground, 0., p00, 0., bias_sigma * bias_sigma]);
            return Ok((ground, root(p00)));
        };
        let (old, mut speed, mut bias, mut p00, mut p01, mut p11) = (
            previous[0],
            previous[1],
            previous[2],
            previous[3],
            previous[4],
            previous[5],
        );
        let dt = time - old;
        let phi = (-dt / c.speed_bias_tau_s).exp();
        bias *= phi;
        p00 += square(c.lead_accel_std_mps2 * dt)?;
        p01 *= phi;
        p11 = phi * phi * p11 + bias_sigma * bias_sigma * (1. - phi * phi);
        for (measurement, noise, h1) in std::iter::once((ground, white * white, 1.)).chain(
            extra
                .iter()
                .map(|(value, sigma)| (*value, sigma * sigma, 0.)),
        ) {
            let (mut ph0, ph1) = (p00 + h1 * p01, p01 + h1 * p11);
            let mut total = ph0 + h1 * ph1 + noise;
            let mut innovation = measurement - (speed + h1 * bias);
            let mut limit = c.innovation_gate_sigma * root(total);
            if c.speed_divergence_inflation && h1 == 0. && limit > 0. && innovation.abs() > limit {
                p00 += square(divide(innovation.abs(), c.innovation_gate_sigma)?)? - total + noise;
                ph0 = p00;
                total = p00 + noise;
                limit = c.innovation_gate_sigma * root(total);
            }
            if c.innovation_gate_sigma > 0. && innovation.abs() > limit {
                innovation = if innovation > 0. { limit } else { -limit };
            }
            let (gain0, gain1) = (divide(ph0, total)?, divide(ph1, total)?);
            speed += gain0 * innovation;
            bias += gain1 * innovation;
            (p00, p01, p11) = (p00 - gain0 * ph0, p01 - gain0 * ph1, p11 - gain1 * ph1);
        }
        self.fused
            .insert(id, vec![time, speed, bias, p00, p01, p11]);
        Ok((speed, root(p00)))
    }
    pub fn range(&mut self, id: u64, time: f64, measured: f64, relative: f64) -> f64 {
        let previous = self.range_est.get(&id);
        let value = match previous {
            Some(previous)
                if relative.is_finite()
                    && previous[2].is_finite()
                    && 0. < time - previous[0]
                    && time - previous[0] <= 0.5 =>
            {
                let mut value = previous[1] + 0.5 * (previous[2] + relative) * (time - previous[0]);
                value += self.config.range_fusion_gain * (measured - value);
                value
            }
            _ => measured,
        };
        self.range_est.insert(id, vec![time, value, relative]);
        value
    }
}
