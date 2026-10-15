//! Image-sample statistics with the final observer's interpolation order.
pub fn percentile(values: &[f64], percent: f64) -> Option<f64> {
    if values.is_empty()
        || !values.iter().all(|v| v.is_finite())
        || !percent.is_finite()
        || !(0. ..=100.).contains(&percent)
    {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let rank = (sorted.len() - 1) as f64 * (percent / 100.);
    let (low, high) = (sorted[rank.floor() as usize], sorted[rank.ceil() as usize]);
    let fraction = rank - rank.floor();
    Some(if fraction >= 0.5 {
        high - (high - low) * (1. - fraction)
    } else {
        low + (high - low) * fraction
    })
}
pub fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() || !values.iter().all(|v| v.is_finite()) {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let n = sorted.len();
    Some(if n.is_multiple_of(2) {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.
    } else {
        sorted[n / 2]
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn percentile_interpolates_either_side_and_median_uses_middle_pair() {
        assert_eq!(percentile(&[0., 10., 20., 30.], 25.), Some(7.5));
        assert_eq!(percentile(&[0., 10., 20., 30.], 75.), Some(22.5));
        assert_eq!(median(&[6., 1., 4., 2.]), Some(3.));
        assert_eq!(percentile(&[], 80.), None);
        assert_eq!(percentile(&[f64::NAN], 80.), None);
    }
}
