use crate::lead::Lead;
use openpilot_control_policy::math::clip;

const fn times() -> [f64; 13] {
    let mut values = [0.; 13];
    let mut index = 0;
    while index <= 12 {
        let ratio = index as f64 / 12.;
        values[index] = 10. * (ratio * ratio);
        index += 1;
    }
    values
}
pub const TIMES: [f64; 13] = times();

pub fn stopped_equivalence(speed: f64) -> f64 {
    speed.powi(2) / (2. * 2.5)
}
pub fn safe_distance(speed: f64, follow: f64, brake: f64, stop: f64) -> f64 {
    speed.powi(2) / (2. * brake) + follow * speed + stop
}
pub fn desired_distance(speed: f64, lead_speed: f64, follow: f64, brake: f64, stop: f64) -> f64 {
    safe_distance(speed, follow, brake, stop) - stopped_equivalence(lead_speed)
}

pub fn extrapolate(distance: f64, speed: f64, acceleration: f64, tau: f64) -> [[f64; 2]; 13] {
    let mut velocity_integral = 0.;
    let mut position_integral = 0.;
    let mut previous_time = 0.;
    std::array::from_fn(|i| {
        let time = TIMES[i];
        let dt = time - previous_time;
        let acceleration = acceleration * (-tau * time.powi(2) / 2.).exp();
        velocity_integral += dt * acceleration;
        let velocity = clip(speed + velocity_integral, 0., 1e8);
        position_integral += dt * velocity;
        previous_time = time;
        [distance + position_integral, velocity]
    })
}

pub fn lead(lead: &Lead, ego_speed: f64) -> ([[f64; 2]; 13], f64) {
    let (distance, speed, acceleration, tau) = if lead.status {
        (lead.d_rel, lead.v_lead, lead.a_lead_k, lead.a_lead_tau)
    } else {
        (50., ego_speed + 10., 0., 1.5)
    };
    let minimum = ((ego_speed + speed) / 2.) * (ego_speed - speed) / (4. * 2.);
    let distance = clip(distance, minimum, 1e8);
    let speed = clip(speed, 0., 1e8);
    let acceleration = clip(acceleration, -10., 5.);
    (extrapolate(distance, speed, acceleration, tau), speed)
}

pub(super) fn numpy_min(values: &[f64]) -> f64 {
    values
        .iter()
        .copied()
        .reduce(|left, right| {
            if left.is_nan() || right.is_nan() {
                f64::NAN
            } else if left < right {
                left
            } else {
                right
            }
        })
        .unwrap_or(f64::INFINITY)
}

pub(super) fn numpy_max_zero(value: f64) -> f64 {
    if value.is_nan() {
        value
    } else {
        value.max(0.)
    }
}

pub(super) fn argmin(values: &[f64]) -> usize {
    let mut index = 0;
    for i in 1..values.len() {
        if values[index].is_nan() {
            break;
        }
        if values[i].is_nan() || values[i] < values[index] {
            index = i;
        }
    }
    index
}
