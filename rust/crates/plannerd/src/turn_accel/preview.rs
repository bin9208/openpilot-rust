use super::TurnInput;
use crate::{model::Model, Error};
use openpilot_control_policy::{drive::TIMES, math::interp};

pub(super) struct Preview {
    pub distances: [f64; 20],
    pub curvatures: [f64; 20],
    pub length: usize,
}

pub(super) fn curvatures(model: &Model, input: &TurnInput) -> Result<Option<Preview>, Error> {
    let values = [
        &model.position.x,
        &model.position.y,
        &model.position.z,
        &model.velocity.x,
        &model.orientation_rate.z,
    ];
    if values.iter().any(|values| values.len() != 33) {
        return Ok(None);
    }
    if values
        .iter()
        .any(|values| values[..19].iter().any(|value| !value.is_finite()))
    {
        return Ok(None);
    }
    let mut sampled = [[0.; 19]; 5];
    for (column, values) in values.iter().enumerate() {
        for index in 0..19 {
            let time = if index == 18 { 3. } else { TIMES[index] };
            sampled[column][index] = interp(time, &TIMES, values)?;
        }
    }
    let [x, y, z, velocity, yaw] = sampled;
    let mut distances = [0.; 19];
    for index in 1..19 {
        let dx = x[index] - x[index - 1];
        let dy = y[index] - y[index - 1];
        let dz = z[index] - z[index - 1];
        distances[index] = distances[index - 1] + (dx.powi(2) + dy.powi(2) + dz.powi(2)).sqrt();
    }
    let reachable = input.speed * 3. + 0.5 * input.acceleration[1] * 3_f64.powi(2);
    let mut result = Preview {
        distances: [0.; 20],
        curvatures: [0.; 20],
        length: 0,
    };
    for index in 0..19 {
        if distances[index] <= reachable && velocity[index] >= 3. {
            result.distances[result.length] = distances[index];
            result.curvatures[result.length] = (yaw[index] / velocity[index]).abs();
            result.length += 1;
        }
    }
    if 0. < reachable && reachable < distances[18] {
        let speed = interp(reachable, &distances, &velocity)?;
        if speed >= 3. {
            result.distances[result.length] = reachable;
            result.curvatures[result.length] = interp(reachable, &distances, &yaw)?.abs() / speed;
            result.length += 1;
        }
    }
    Ok(if result.length == 0 {
        None
    } else {
        Some(result)
    })
}
