use crate::{
    scalar::{maximum, minimum, square},
    Error,
};
use serde::Serialize;

#[derive(Clone, Serialize)]
pub struct LeadFilter {
    #[serde(serialize_with = "crate::scalar::serialize_float")]
    pub dt: f64,
    #[serde(serialize_with = "crate::scalar::serialize_float")]
    pub alpha_slow: f64,
    #[serde(serialize_with = "crate::scalar::serialize_float")]
    pub beta_slow: f64,
    #[serde(serialize_with = "crate::scalar::serialize_float")]
    pub alpha_range: f64,
    #[serde(serialize_with = "crate::scalar::serialize_float")]
    pub beta_range: f64,
    #[serde(serialize_with = "crate::scalar::serialize_float")]
    pub residual_alpha: f64,
    #[serde(serialize_with = "crate::scalar::serialize_float")]
    pub noise_alpha: f64,
    #[serde(serialize_with = "crate::scalar::serialize_float")]
    pub accel_alpha_range: f64,
    #[serde(serialize_with = "crate::scalar::serialize_float")]
    pub residual_scale_squared: f64,
    #[serde(serialize_with = "crate::scalar::serialize_float")]
    pub max_accel_step: f64,
    #[serde(serialize_with = "crate::scalar::serialize_float")]
    pub velocity: f64,
    #[serde(serialize_with = "crate::scalar::serialize_float")]
    pub acceleration: f64,
    #[serde(serialize_with = "crate::scalar::serialize_float")]
    pub mean_residual: f64,
    #[serde(serialize_with = "crate::scalar::serialize_float")]
    pub previous_residual: f64,
    #[serde(serialize_with = "crate::scalar::serialize_float")]
    pub residual_variance: f64,
    #[serde(serialize_with = "crate::scalar::serialize_float")]
    pub response_weight: f64,
    pub limited: bool,
}

impl LeadFilter {
    pub fn new(velocity: f64, dt: f64) -> Result<Self, Error> {
        if !dt.is_finite() || dt <= 0. {
            return Err(Error::InvalidPeriod);
        }
        let slow_v = 0.10 / (0.10 + dt);
        let slow_a = 0.15 / (0.15 + dt);
        let fast_v = 0.05 / (0.05 + dt);
        let fast_a = 0.075 / (0.075 + dt);
        let alpha_slow = 1. - slow_v * slow_a;
        let beta_slow = (1. - slow_v) * (1. - slow_a);
        let residual_alpha = dt / (0.15 + dt);
        let mut state = Self {
            dt,
            alpha_slow,
            beta_slow,
            alpha_range: 1. - fast_v * fast_a - alpha_slow,
            beta_range: (1. - fast_v) * (1. - fast_a) - beta_slow,
            residual_alpha,
            noise_alpha: dt / (0.50 + dt),
            accel_alpha_range: dt / (0.075 + dt) - residual_alpha,
            residual_scale_squared: square(0.10 + 0.8 * dt)?,
            max_accel_step: 3. * residual_alpha,
            velocity: 0.,
            acceleration: 0.,
            mean_residual: 0.,
            previous_residual: 0.,
            residual_variance: 0.,
            response_weight: 0.,
            limited: false,
        };
        state.reset(velocity);
        Ok(state)
    }

    pub fn reset(&mut self, velocity: f64) {
        self.velocity = velocity;
        self.acceleration = 0.;
        self.mean_residual = 0.;
        self.previous_residual = 0.;
        self.residual_variance = 0.;
        self.response_weight = 0.;
        self.limited = false;
    }

    pub fn update(&mut self, velocity: f64, stationary: bool) -> Result<f64, Error> {
        let predicted_velocity = self.velocity + self.acceleration * self.dt;
        let residual = velocity - predicted_velocity;
        let residual_sample = if stationary {
            0.
        } else {
            maximum(-0.5, minimum(0.5, residual))
        };
        let evidence = maximum(0., self.mean_residual * residual_sample);
        let noise_sample = 0.5 * square(residual_sample - self.previous_residual)?;
        self.previous_residual = residual_sample;
        self.residual_variance += self.noise_alpha * (noise_sample - self.residual_variance);
        self.mean_residual += self.residual_alpha * (residual_sample - self.mean_residual);
        self.response_weight = square(
            evidence / (self.residual_scale_squared + 8. * self.residual_variance + evidence),
        )?;
        let alpha = self.alpha_slow + self.alpha_range * self.response_weight;
        let beta = self.beta_slow + self.beta_range * self.response_weight;
        let predicted_acceleration = self.acceleration + beta / self.dt * residual;
        let correction = if stationary {
            -self.residual_alpha * self.acceleration
        } else {
            beta / self.dt * residual
        };
        let accel_alpha = self.residual_alpha + self.accel_alpha_range * self.response_weight;
        let correction = maximum(
            (-10. - self.acceleration) * accel_alpha,
            minimum((5. - self.acceleration) * accel_alpha, correction),
        );
        self.limited = correction.abs() > self.max_accel_step;
        let correction = maximum(
            -self.max_accel_step,
            minimum(self.max_accel_step, correction),
        );
        self.acceleration = maximum(-10., minimum(5., self.acceleration + correction));
        let speed_rc = 0.10 - 0.05 * self.response_weight;
        self.velocity = predicted_velocity
            + alpha * residual
            + speed_rc * (self.acceleration - predicted_acceleration);
        Ok(self.acceleration)
    }
}
