use crate::{sum::pairwise, Error};
use num_traits::ToPrimitive;
pub fn masked(values: &[f64], mask: &[bool], kernel: (usize, f64)) -> Result<Vec<f64>, Error> {
    let (size, sigma) = kernel;
    if size == 0 || size % 2 == 0 || values.is_empty() || values.len() != mask.len() {
        return Err(Error::Contract("smoothing shape/kernel"));
    }
    let pad = size / 2;
    let center = pad.to_f64().ok_or(Error::Contract("kernel size"))?;
    let mut weights = (0..size)
        .map(|index| {
            let value = (index.to_f64().ok_or(Error::Contract("kernel index"))? - center) / sigma;
            Ok((-0.5 * value.powi(2)).exp())
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let total = pairwise(&weights);
    weights.iter_mut().for_each(|weight| *weight /= total);
    let mut output = Vec::with_capacity(values.len());
    for index in 0..values.len() {
        let mut numerator = 0.;
        let mut denominator = 0.;
        for (offset, &weight) in weights.iter().rev().enumerate() {
            let at = (index + offset).saturating_sub(pad).min(values.len() - 1);
            // Preserve NaN * 0 from the source's x * mask before convolution.
            numerator += values[at] * f64::from(mask[at]) * weight;
            denominator += f64::from(mask[at]) * weight;
        }
        output.push(if denominator != 0. {
            numerator / denominator
        } else {
            f64::NAN
        });
    }
    Ok(output)
}
