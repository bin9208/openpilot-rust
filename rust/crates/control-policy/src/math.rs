use crate::Error;

pub fn clip(value: f64, low: f64, high: f64) -> f64 {
    if value.is_nan() || low.is_nan() || high.is_nan() {
        return f64::NAN;
    }
    let value = if value < low { low } else { value };
    if value > high {
        high
    } else {
        value
    }
}
pub fn maximum(a: f64, b: f64) -> f64 {
    if b > a {
        b
    } else {
        a
    }
}
pub fn minimum(a: f64, b: f64) -> f64 {
    if b < a {
        b
    } else {
        a
    }
}
pub fn sign(value: f64) -> f64 {
    if value.is_nan() {
        f64::NAN
    } else if value > 0. {
        1.
    } else if value < 0. {
        -1.
    } else {
        0.
    }
}
pub fn divide(numerator: f64, denominator: f64) -> Result<f64, Error> {
    if denominator == 0. {
        Err(Error::Contract("division by zero"))
    } else {
        Ok(numerator / denominator)
    }
}
pub fn interp(query: f64, x: &[f64], y: &[f64]) -> Result<f64, Error> {
    if x.is_empty() || x.len() != y.len() {
        return Err(Error::Contract("interpolation dimensions"));
    }
    if x.len() == 1 || query < x[0] {
        return Ok(y[0]);
    }
    if query >= x[x.len() - 1] {
        return Ok(y[y.len() - 1]);
    }
    if query.is_nan() {
        return Ok(f64::NAN);
    }
    let next = x.partition_point(|value| *value <= query);
    if next == 0 {
        return Ok(y[0]);
    }
    if next == x.len() {
        return Ok(y[y.len() - 1]);
    }
    let i = next - 1;
    if x[i] == query {
        return Ok(y[i]);
    }
    let slope = (y[next] - y[i]) / (x[next] - x[i]);
    let mut result = slope * (query - x[i]) + y[i];
    if result.is_nan() {
        result = slope * (query - x[next]) + y[next];
        if result.is_nan() && y[i] == y[next] {
            result = y[i];
        }
    }
    Ok(result)
}
pub fn smooth(value: f64, previous: f64, tau: f64) -> f64 {
    let alpha = if tau > 0. {
        1. - (-0.01 / tau).exp()
    } else {
        1.
    };
    alpha * value + (1. - alpha) * previous
}
