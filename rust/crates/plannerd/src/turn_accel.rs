use crate::{model::Model, Error};
use openpilot_control_policy::{
    drive::TIMES,
    math::{interp, maximum, minimum},
};
use serde::Deserialize;

mod preview;

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnInput {
    pub speed: f64,
    pub curvature: f64,
    pub acceleration: [f64; 2],
    pub lateral_maximum: f64,
    pub safety_ratio: f64,
    pub minimum_speed: f64,
    pub cruise_speed: Option<f64>,
    pub current_curvature: f64,
}

pub fn future_curvature(model: &Model, fallback: f64, lookahead: f64) -> Result<f64, Error> {
    if model.orientation_rate.z.len() != 33 || model.velocity.x.len() != 33 {
        return Ok(fallback);
    }
    let yaw = interp(lookahead, &TIMES, &model.orientation_rate.z)?;
    let velocity = interp(lookahead, &TIMES, &model.velocity.x)?;
    if !yaw.is_finite() || !velocity.is_finite() {
        return Ok(fallback);
    }
    Ok(yaw / maximum(velocity.abs(), 3.))
}

pub fn limit(input: &TurnInput, model: Option<&Model>) -> Result<[f64; 2], Error> {
    if input.speed < input.minimum_speed
        || input.lateral_maximum <= 0.
        || input.acceleration[1] <= 0.
    {
        return Ok(input.acceleration);
    }
    let total = input.lateral_maximum.abs() * input.safety_ratio;
    let curvature = [input.curvature, input.current_curvature]
        .into_iter()
        .filter(|value| value.is_finite())
        .map(f64::abs)
        .fold(0., maximum);
    let lateral = input.speed.powi(2) * curvature;
    let mut ceiling = minimum(
        input.acceleration[1],
        maximum(0., total.powi(2) - lateral.powi(2)).sqrt(),
    );
    if model.is_some() && total > 0. {
        ceiling = minimum(
            ceiling,
            input.acceleration[1] * maximum(0., 1. - (lateral / total).powi(2)),
        );
    }
    if ceiling <= 0. {
        return Ok([input.acceleration[0], ceiling]);
    }
    let preview = match model {
        Some(model) => preview::curvatures(model, input)?,
        None => None,
    };
    let Some(preview) = preview else {
        return Ok([input.acceleration[0], ceiling]);
    };
    let cruise_squared = match input.cruise_speed {
        Some(speed) if speed.is_finite() => maximum(input.speed, speed).powi(2),
        Some(_) | None => f64::INFINITY,
    };
    let ego_squared = input.speed.powi(2);
    let total_squared = total.powi(2);
    let feasible = |acceleration: f64| {
        let mut peak = f64::NEG_INFINITY;
        for index in 0..preview.length {
            let speed_squared = ego_squared + 2. * acceleration * preview.distances[index];
            let speed_squared = if speed_squared.is_nan() {
                f64::NAN
            } else {
                minimum(speed_squared, cruise_squared)
            };
            let lateral = speed_squared * preview.curvatures[index];
            peak = if peak.is_nan() || lateral.is_nan() {
                f64::NAN
            } else {
                maximum(peak, lateral)
            };
        }
        let lateral_squared = peak * peak;
        acceleration.powi(2) + lateral_squared <= total_squared
            && acceleration <= input.acceleration[1] * (1. - lateral_squared / total_squared)
    };
    if !feasible(0.) {
        return Ok([input.acceleration[0], 0.]);
    }
    if feasible(ceiling) {
        return Ok([input.acceleration[0], ceiling]);
    }
    let mut lower = 0.;
    let mut upper = ceiling;
    for _ in 0..12 {
        let middle = (lower + upper) * 0.5;
        if feasible(middle) {
            lower = middle;
        } else {
            upper = middle;
        }
    }
    Ok([input.acceleration[0], lower])
}
