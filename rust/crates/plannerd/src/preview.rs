use crate::driving_mode::DrivingMode;
use openpilot_control_policy::math::{maximum, minimum};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewInput {
    pub lead_status: bool,
    pub lead_acceleration: f64,
    pub ego_acceleration: f64,
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct PreviewRequest {
    pub offset_s: f64,
    pub lead_accel_signal: f64,
    pub active: bool,
}

pub(crate) fn acceleration_signal(lead: f64, ego: f64) -> f64 {
    let bounded_ego = maximum(-3., minimum(3., ego));
    let value = lead - bounded_ego;
    if value > 0.10 {
        value - 0.10
    } else if value < -0.10 {
        value + 0.10
    } else {
        0.
    }
}

pub fn request(input: PreviewInput) -> PreviewRequest {
    if !input.lead_status
        || ![input.lead_acceleration, input.ego_acceleration]
            .iter()
            .all(|value| value.is_finite())
    {
        return PreviewRequest::default();
    }
    let signal = acceleration_signal(input.lead_acceleration, input.ego_acceleration);
    PreviewRequest {
        offset_s: if signal < 0. {
            minimum(-signal * 1., 1.50)
        } else {
            0.
        },
        lead_accel_signal: signal,
        active: true,
    }
}

pub fn rate_limit(target: f64, current: f64) -> f64 {
    let step = if target > current && target > 0. {
        0.08
    } else {
        0.03
    };
    maximum(current - step, minimum(current + step, target))
}

pub fn clip_offset(base: f64, preview: f64) -> f64 {
    maximum(0.05, minimum(2.50, base + preview)) - base
}

pub fn apply_target(base: f64, preview: f64, mode: DrivingMode) -> f64 {
    let [floor, delta] = match mode {
        DrivingMode::Safe => [-0.05, 0.10],
        DrivingMode::Eco => [-0.04, 0.09],
        DrivingMode::Normal | DrivingMode::High => [-0.03, 0.08],
    };
    let candidate = minimum(preview, base);
    if base > 0. {
        maximum(candidate, floor)
    } else {
        maximum(candidate, base - delta)
    }
}
