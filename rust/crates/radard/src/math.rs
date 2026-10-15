use num_traits::ToPrimitive;
pub use openpilot_control_policy::math::{maximum, minimum};
pub use openpilot_radarcan::scalar::{float_sum, square};

pub fn count(value: usize) -> Result<f64, crate::Error> {
    value.to_f64().ok_or(crate::Error::Contract(
        "sample count exceeds floating-point range",
    ))
}

pub fn finite(value: f64, fallback: f64) -> f64 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

pub fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let middle = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        (sorted[middle - 1] + sorted[middle]) / 2.
    } else {
        sorted[middle]
    }
}

pub fn robust_center_and_sigma(values: &[f64]) -> (f64, f64) {
    let center = median(values);
    let deviations: Vec<_> = values.iter().map(|value| (value - center).abs()).collect();
    (center, 1.4826 * median(&deviations))
}

pub use openpilot_radarcan::scalar::norm;
