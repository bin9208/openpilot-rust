use super::history::{directional_metrics, linear_fit, spatial_fit, Observation, Track};
use crate::{
    math::{maximum, minimum, robust_center_and_sigma, square},
    Error,
};

pub fn continuous(track: &Track, observation: &Observation) -> Result<bool, Error> {
    let Some(previous) = track.observations.back() else {
        return Ok(true);
    };
    let dt = observation.time_s - previous.time_s;
    if dt <= 0. || dt > 0.35 {
        return Ok(false);
    }
    let [slope, _, _] = spatial_fit(&track.window(0.45))?;
    let rate = slope * previous.path_velocity;
    let longitudinal = previous.path_x_world + previous.path_velocity * dt;
    let lateral = previous.d_path + rate * dt;
    let longitudinal_limit = 1.75 + 0.25 * previous.path_velocity.abs() * dt;
    let lateral_limit = 0.85 + 0.25 * rate.abs() * dt;
    Ok(
        (observation.path_x_world - longitudinal).abs() <= longitudinal_limit
            && (observation.d_path - lateral).abs() <= lateral_limit
            && (observation.v_rel - previous.v_rel).abs() <= 5.,
    )
}

fn interval_probability(mean: f64, sigma: f64) -> f64 {
    let scale = maximum(sigma, 0.05) * 2_f64.sqrt();
    let upper = libm::erf((1.8 - mean) / scale);
    let lower = libm::erf((-1.8 - mean) / scale);
    minimum(1., maximum(0., 0.5 * (upper - lower)))
}

pub fn probability(track: &mut Track, observation: &Observation) -> Result<f64, Error> {
    let short = track.window(0.45);
    let long = track.window(1.50);
    let [mut short_slope, short_residual, short_span] = spatial_fit(&short)?;
    let [mut long_slope, long_residual, long_span] = spatial_fit(&long)?;
    let [displacement, consistency, inward_ratio] = directional_metrics(track, observation.d_path)?;
    let [speed_long, _] = linear_fit(&long, |value| value.path_x_world)?;
    let reported_speed = robust_center_and_sigma(
        &long
            .iter()
            .map(|value| value.path_velocity)
            .collect::<Vec<_>>(),
    )
    .0;
    let enough = long.len() >= 4 && long_span >= 1.5;
    let speed = if long.len() >= 2 {
        0.70 * speed_long + 0.30 * reported_speed
    } else {
        observation.path_velocity
    };
    if !enough {
        short_slope = 0.;
        long_slope = 0.;
    }
    let short_rate = short_slope * speed;
    let long_rate = long_slope * speed;
    let curvature_span = maximum(1., long_span - 0.5 * short_span);
    let curvature = maximum(
        -0.5,
        minimum(0.5, (short_slope - long_slope) / curvature_span),
    );
    let position_rate = long_slope * speed;
    let mut rate = position_rate;
    let side = if observation.d_path.abs() > 1e-6 {
        1_f64.copysign(observation.d_path)
    } else {
        0.
    };
    track.position_history_override_latched = false;
    if enough
        && displacement >= 0.20
        && consistency >= 0.75
        && inward_ratio >= 0.65
        && -side * short_rate >= 0.15
        && -side * long_rate >= 0.10
    {
        let responsive = (1. - 0.50) * position_rate + 0.50 * short_rate;
        if -side * responsive > -side * long_rate {
            rate = responsive;
        }
    }
    let slope = if speed.abs() > 0.1 { rate / speed } else { 0. };
    let disagreement = (short_slope - long_slope).abs();
    let uncertainty = 0.20 + long_residual + 0.5 * short_residual + if enough { 0. } else { 0.45 };
    track.occupancy(observation, enough);
    if !(enough && track.inside_latched) {
        return Ok(0.);
    }
    let acceleration = maximum(-3., minimum(3., observation.a_lead));
    let mut probability = 0.;
    for horizon in [0.5, 1., 1.5, 2., 2.5, 3., 3.5, 4., 4.5, 5.] {
        let displacement = speed * horizon + 0.5 * acceleration * square(horizon)?;
        let future_distance = (observation.path_s + displacement)
            - (observation.ego_path_s + observation.v_ego * horizon);
        let future_lateral = observation.d_path + slope * displacement;
        let sigma = uncertainty
            + disagreement * displacement.abs()
            + 0.20 * curvature.abs() * square(displacement)?;
        let support = minimum(1., long_span / maximum(displacement.abs(), 1.5));
        let occupancy = if future_distance > 0. {
            interval_probability(future_lateral, sigma) * support
        } else {
            0.
        };
        if future_lateral.abs() > observation.d_path.abs() {
            probability = maximum(probability, 1. - occupancy);
        }
    }
    Ok(probability)
}
