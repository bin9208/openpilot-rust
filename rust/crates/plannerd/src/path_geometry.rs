use crate::Error;
use num_traits::ToPrimitive;
use openpilot_control_policy::math::clip;

pub const POINTS: usize = 33;

#[derive(Debug)]
pub struct YawPlan {
    pub yaw: [f64; POINTS],
    pub rate: [f64; POINTS],
}

fn smooth(values: &[f64; POINTS], window: usize) -> [f64; POINTS] {
    let divisor = if window == 5 {
        5.
    } else if window == 7 {
        7.
    } else {
        9.
    };
    let coefficient = 1. / divisor;
    let padding = window / 2;
    std::array::from_fn(|i| {
        (0..window)
            .map(|j| values[(i + j).saturating_sub(padding).min(POINTS - 1)] * coefficient)
            .fold(0., |total, value| total + value)
    })
}

fn gradient(values: &[f64; POINTS], distance: &[f64; POINTS]) -> [f64; POINTS] {
    let steps: [f64; POINTS - 1] = std::array::from_fn(|i| distance[i + 1] - distance[i]);
    let uniform = steps.iter().all(|step| *step == steps[0]);
    std::array::from_fn(|i| {
        if i == 0 {
            (values[1] - values[0]) / steps[0]
        } else if i == POINTS - 1 {
            (values[i] - values[i - 1]) / steps[i - 1]
        } else if uniform {
            (values[i + 1] - values[i - 1]) / (2. * steps[0])
        } else {
            let left = steps[i - 1];
            let right = steps[i];
            let a = -right / (left * (left + right));
            let b = (right - left) / (left * right);
            let c = left / (right * (left + right));
            a * values[i - 1] + b * values[i] + c * values[i + 1]
        }
    })
}

fn unwrap(yaw: &mut [f64; POINTS]) {
    let pi = std::f64::consts::PI;
    let period = 2. * pi;
    let mut previous = yaw[0];
    let mut total = 0.;
    for value in &mut yaw[1..] {
        let difference = *value - previous;
        previous = *value;
        let correction = if difference.abs() < pi {
            0.
        } else {
            let remainder = (difference + pi) % period;
            let remainder = if remainder == 0. {
                0.
            } else if remainder < 0. {
                remainder + period
            } else {
                remainder
            };
            let mut wrapped = remainder - pi;
            if wrapped == -pi && difference > 0. {
                wrapped = pi;
            }
            wrapped - difference
        };
        total += correction;
        *value += total;
    }
}

pub fn yaw_from_path(path: &[[f64; 3]; POINTS], speeds: &[f64; POINTS]) -> Result<YawPlan, Error> {
    let x = path.map(|point| point[0]);
    let y = path.map(|point| point[1]);
    let mut distance = [0.; POINTS];
    for i in 1..POINTS {
        let dx = x[i] - x[i - 1];
        let dy = y[i] - y[i - 1];
        let segment = (dx * dx + dy * dy).sqrt();
        distance[i] = distance[i - 1] + if segment < 0.05 { 0.05 } else { segment };
    }
    let window = if speeds[0] <= 6. { 9 } else { 5 };
    let dx = gradient(&smooth(&x, window), &distance);
    let dy = gradient(&smooth(&y, window), &distance);
    let d2x = gradient(&dx, &distance);
    let d2y = gradient(&dy, &distance);
    let mut yaw = std::array::from_fn(|i| dy[i].atan2(dx[i]));
    unwrap(&mut yaw);
    let mut rate = std::array::from_fn(|i| {
        let denominator = (dx[i] * dx[i] + dy[i] * dy[i]).powf(1.5);
        let denominator = if denominator < 1e-9 {
            1e-9
        } else {
            denominator
        };
        ((dx[i] * d2y[i] - dy[i] * d2x[i]) / denominator) * speeds[i]
    });
    if speeds[0] <= 6. {
        rate = smooth(&rate, 7);
    }
    for value in &mut yaw {
        let stabilized = if value.is_finite() { *value } else { 0. };
        *value = f64::from(
            stabilized
                .to_f32()
                .ok_or(Error::Contract("yaw float32 conversion"))?,
        );
    }
    for value in &mut rate {
        let stabilized = clip(if value.is_finite() { *value } else { 0. }, -2., 2.);
        *value = f64::from(
            stabilized
                .to_f32()
                .ok_or(Error::Contract("yaw rate float32 conversion"))?,
        );
    }
    Ok(YawPlan { yaw, rate })
}
