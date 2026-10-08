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

// CPython 3.12.14 vector_norm preserves the source's compensated rounding;
// system hypot changes path tangents by an ULP. See the retained PSF provenance.
pub fn norm(values: &[f64]) -> f64 {
    let maximum = values
        .iter()
        .fold(0_f64, |total, value| maximum(total, value.abs()));
    if maximum.is_infinite() {
        return maximum;
    }
    if values.iter().any(|value| value.is_nan()) {
        return f64::NAN;
    }
    if maximum == 0. || values.len() <= 1 {
        return maximum;
    }
    let (_, exponent) = libm::frexp(maximum);
    if exponent < -1023 {
        let normalized: Vec<_> = values
            .iter()
            .map(|value| value / f64::MIN_POSITIVE)
            .collect();
        return f64::MIN_POSITIVE * norm(&normalized);
    }
    let scale = libm::ldexp(1., -exponent);
    let mut sum = 1.;
    let mut fraction1 = 0.;
    let mut fraction2 = 0.;
    for value in values {
        let x = value.abs() * scale;
        let product = x * x;
        let product_error = x.mul_add(x, -product);
        let next = sum + product;
        fraction1 += product_error;
        fraction2 += (sum - next) + product;
        sum = next;
    }
    let mut result = (sum - 1. + (fraction1 + fraction2)).sqrt();
    let product = -result * result;
    let product_error = (-result).mul_add(result, -product);
    let next = sum + product;
    fraction1 += product_error;
    fraction2 += (sum - next) + product;
    sum = next;
    let residual = sum - 1. + (fraction1 + fraction2);
    result += residual / (2. * result);
    result / scale
}
