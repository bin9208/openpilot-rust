use crate::Error;
use num_traits::ToPrimitive;
pub use openpilot_ui_framework::text_layout::float;

pub fn clip(value: f64, low: f64, high: f64) -> f64 {
    if value.is_nan() {
        value
    } else if value < low {
        low.min(high)
    } else if value > high {
        high
    } else {
        value
    }
}
pub fn integer(value: f64) -> Result<i32, Error> {
    value
        .to_i32()
        .ok_or(Error::Contract("model render coordinate outside i32"))
}
pub fn byte(value: f64) -> Result<u8, Error> {
    value
        .trunc()
        .to_u8()
        .ok_or(Error::Contract("model render color outside u8"))
}
pub fn interp(value: f64, nodes: &[f64], values: &[f64]) -> Result<f64, Error> {
    if nodes.is_empty() || nodes.len() != values.len() {
        return Err(Error::Contract("model interpolation dimensions"));
    }
    if value.is_nan() {
        return Ok(value);
    }
    if value > nodes[nodes.len() - 1] {
        return Ok(values[values.len() - 1]);
    }
    if value < nodes[0] {
        return Ok(values[0]);
    }
    let (mut low, mut high) = (0, nodes.len());
    while low < high {
        let middle = (low + high) / 2;
        if value >= nodes[middle] {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    let index = low.saturating_sub(1);
    if index == nodes.len() - 1 || value == nodes[index] {
        return Ok(values[index]);
    }
    let slope = (values[index + 1] - values[index]) / (nodes[index + 1] - nodes[index]);
    let mut output = slope * (value - nodes[index]) + values[index];
    if output.is_nan() {
        output = slope * (value - nodes[index + 1]) + values[index + 1];
        if output.is_nan() && values[index] == values[index + 1] {
            output = values[index];
        }
    }
    Ok(output)
}
pub fn index(line: &[super::points::ModelPoint], distance: f64) -> usize {
    line.iter()
        .rposition(|point| f64::from(point.0[0]) <= distance)
        .unwrap_or(0)
}

pub fn decimal(value: f64, precision: usize) -> String {
    if value.is_nan() {
        "nan".into()
    } else {
        format!("{value:.precision$}")
    }
}
