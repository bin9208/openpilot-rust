use crate::{
    math::{clip, interp, maximum},
    Error,
};

pub const CONTROL_N: usize = 17;
pub const TIMES: [f64; 33] = [
    0.,
    0.009765625,
    0.0390625,
    0.087890625,
    0.15625,
    0.244140625,
    0.3515625,
    0.478515625,
    0.625,
    0.791015625,
    0.9765625,
    1.181640625,
    1.40625,
    1.650390625,
    1.9140625,
    2.197265625,
    2.5,
    2.822265625,
    3.1640625,
    3.525390625,
    3.90625,
    4.306640625,
    4.7265625,
    5.166015625,
    5.625,
    6.103515625,
    6.6015625,
    7.119140625,
    7.65625,
    8.212890625,
    8.7890625,
    9.384765625,
    10.,
];

pub fn steer_ratio(live: f64, rate: f64, custom: f64, meb: bool) -> f64 {
    let live = maximum(live, 0.1);
    if meb {
        return live;
    }
    let custom = custom * 0.1;
    if custom.is_finite() && custom > 1. {
        return custom;
    }
    let rate = if !rate.is_finite() || !(30. ..=200.).contains(&rate) {
        100.
    } else {
        rate
    };
    maximum(live * rate * 0.01, 0.1)
}

pub struct Plan<'a> {
    pub psis: &'a [f64],
    pub curvatures: &'a [f64],
    pub distances: &'a [f64],
}
pub fn lag_adjusted(speed: f64, delay: f64, plan: Plan<'_>) -> Result<f64, Error> {
    let zero = [0.; CONTROL_N];
    let plan = if plan.psis.len() != CONTROL_N {
        Plan {
            psis: &zero,
            curvatures: &zero,
            distances: &zero,
        }
    } else {
        plan
    };
    let speed = maximum(1., speed);
    let delay = maximum(0.01, delay);
    let first = *plan
        .curvatures
        .first()
        .ok_or(Error::Contract("empty lateral curvature"))?;
    interp(delay, &TIMES[..CONTROL_N], plan.curvatures)?;
    interp(1.2, &TIMES[..CONTROL_N], plan.curvatures)?;
    let psi = interp(delay, &TIMES[..CONTROL_N], plan.psis)?;
    let distance = maximum(interp(delay, &TIMES[..CONTROL_N], plan.distances)?, 0.001);
    let desired = 2. * (psi / distance) - first;
    let rate = 5. / speed.powi(2);
    Ok(clip(desired, first - rate * 0.05, first + rate * 0.05))
}
pub fn clip_curvature(
    speed: f64,
    previous: f64,
    new: f64,
    roll: f64,
) -> Result<(f64, bool), Error> {
    let speed = maximum(speed, 1.);
    let rate = 5. / speed.powi(2);
    let rate_limited = clip(new, previous - rate * 0.01, previous + rate * 0.01);
    let allowed = interp(speed, &[80. / 3.6, 120. / 3.6], &[4.5, 3.])?;
    let low = (-allowed + roll * 9.81) / speed.powi(2);
    let high = (allowed + roll * 9.81) / speed.powi(2);
    let accel = clip(rate_limited, low, high);
    let final_value = clip(accel, -0.2, 0.2);
    Ok((final_value, accel != rate_limited || final_value != accel))
}
