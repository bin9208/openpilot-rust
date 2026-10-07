use crate::{preview::acceleration_signal, types::MpcSource, Error};
use openpilot_control_policy::math::minimum;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy)]
struct Tuning {
    horizon: f64,
    closing_floor: f64,
    accel_cost: f64,
    jerk_cost: f64,
    rise_time: f64,
    gap_fade: f64,
    accel_fade: f64,
}

fn tuning(level: i32) -> Option<Tuning> {
    let [horizon, closing_floor, accel_cost, jerk_cost, rise_time, gap_fade, accel_fade] =
        match level {
            1 => [0.00, 0.00, 0.95, 0.95, 0.80, 2.00, 0.50],
            2 => [0.10, -0.05, 0.85, 0.85, 0.60, 1.50, 0.40],
            3 => [0.25, -0.10, 0.65, 0.70, 0.40, 1.00, 0.30],
            4 => [0.40, -0.15, 0.18, 0.35, 0.15, 0.50, 0.15],
            5 => [0.50, -0.20, 0.05, 0.15, 0.00, 0.00, 0.00],
            _ => return None,
        };
    Some(Tuning {
        horizon,
        closing_floor,
        accel_cost,
        jerk_cost,
        rise_time,
        gap_fade,
        accel_fade,
    })
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResponseInput {
    pub level: i32,
    pub enabled: bool,
    pub source: MpcSource,
    pub lead_status: bool,
    pub lead_acceleration: f64,
    pub ego_acceleration: f64,
    pub relative_speed: f64,
    pub gap_margin: f64,
    pub speed_error: f64,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct LeadRequest {
    pub active: bool,
    pub level: i32,
    pub lead_accel_signal: f64,
    pub a_change_cost_factor: f64,
    pub jerk_cost_factor: f64,
    pub strength: f64,
}

impl Default for LeadRequest {
    fn default() -> Self {
        Self {
            active: false,
            level: 0,
            lead_accel_signal: 0.,
            a_change_cost_factor: 1.,
            jerk_cost_factor: 1.,
            strength: 0.,
        }
    }
}

pub fn request(input: ResponseInput) -> LeadRequest {
    let level = input.level.clamp(0, 5);
    let (allowed, cruise) = match input.source {
        MpcSource::Lead0 | MpcSource::Lead1 => (true, false),
        MpcSource::Cruise => (level >= 3, true),
        MpcSource::E2e => (false, false),
    };
    if !input.enabled
        || !input.lead_status
        || !allowed
        || ![
            input.lead_acceleration,
            input.ego_acceleration,
            input.relative_speed,
            input.gap_margin,
        ]
        .iter()
        .all(|value| value.is_finite())
    {
        return LeadRequest::default();
    }
    let signal = acceleration_signal(input.lead_acceleration, input.ego_acceleration);
    let inactive = LeadRequest {
        level,
        lead_accel_signal: signal,
        ..LeadRequest::default()
    };
    if cruise && (!input.speed_error.is_finite() || input.speed_error <= 1. / 3.6) {
        return inactive;
    }
    let Some(tuning) = tuning(level) else {
        return inactive;
    };
    if input.gap_margin <= 0.
        || input.relative_speed < tuning.closing_floor
        || input.lead_acceleration <= 0.10
        || (level < 5 && !cruise && signal <= 0.)
        || input.relative_speed + signal * tuning.horizon < 0.
    {
        return inactive;
    }
    let strength = if tuning.gap_fade > 0. {
        let signal = if cruise {
            input.lead_acceleration - 0.10
        } else {
            minimum(input.lead_acceleration - 0.10, signal)
        };
        minimum(
            minimum(1., input.gap_margin / tuning.gap_fade),
            signal / tuning.accel_fade,
        )
    } else {
        1.
    };
    LeadRequest {
        active: true,
        level,
        lead_accel_signal: signal,
        strength,
        a_change_cost_factor: if strength == 1. {
            tuning.accel_cost
        } else {
            1. - strength * (1. - tuning.accel_cost)
        },
        jerk_cost_factor: if strength == 1. {
            tuning.jerk_cost
        } else {
            1. - strength * (1. - tuning.jerk_cost)
        },
    }
}

#[derive(Debug, Default)]
pub struct LeadResponseState {
    strength: f64,
    key: Option<(i32, i32)>,
}

impl LeadResponseState {
    pub fn update(
        &mut self,
        request: LeadRequest,
        dt: f64,
        track_id: i32,
    ) -> Result<LeadRequest, Error> {
        let key = (request.level, track_id);
        if !request.active || track_id < 0 || !dt.is_finite() || dt <= 0. {
            self.strength = 0.;
            self.key = None;
            return Ok(LeadRequest::default());
        }
        if self.key != Some(key) {
            self.strength = 0.;
        }
        self.key = Some(key);
        let tuning = tuning(request.level).ok_or(Error::Contract("active lead response level"))?;
        if request.level == 5 {
            self.strength = request.strength;
            return Ok(request);
        }
        self.strength = if tuning.rise_time > 0. {
            minimum(request.strength, self.strength + dt / tuning.rise_time)
        } else {
            request.strength
        };
        Ok(LeadRequest {
            strength: self.strength,
            a_change_cost_factor: 1. - self.strength * (1. - tuning.accel_cost),
            jerk_cost_factor: 1. - self.strength * (1. - tuning.jerk_cost),
            ..request
        })
    }
}
