//! Scalar NumPy operations in the original lagd helper, with pinned external FFT.
use super::normalized;
use crate::seal::Result;
use num_bigint::BigInt;
use openpilot_pocketfft::{Complex, Transform};
use serde_json::{json, Value};

pub(super) fn good_size(n: BigInt) -> BigInt {
    if n <= BigInt::from(6) {
        return n;
    }
    let mut best = &n * 2u8;
    let mut f2 = BigInt::from(1);
    while f2 < best {
        let mut f23 = f2.clone();
        while f23 < best {
            let mut f235 = f23.clone();
            while f235 < best {
                let mut f2357 = f235.clone();
                while f2357 < best {
                    let mut f235711 = f2357.clone();
                    while f235711 < best {
                        if f235711 >= n {
                            best = f235711.clone();
                        }
                        f235711 *= 11u8;
                    }
                    f2357 *= 7u8;
                }
                f235 *= 5u8;
            }
            f23 *= 3u8;
        }
        f2 *= 2u8;
    }
    best
}

// NumPy's contiguous floating pairwise reduction. Keep its eight accumulators
// and recursive split so cancellation is not changed by an algebraic rewrite.
pub(crate) fn sum(values: &[f64]) -> f64 {
    let count = values.len();
    if count < 8 {
        return values.iter().fold(-0.0, |total, value| total + value);
    }
    if count <= 128 {
        let mut lanes = [0.; 8];
        lanes.copy_from_slice(&values[..8]);
        let limit = count - count % 8;
        for chunk in values[8..limit].chunks_exact(8) {
            for index in 0..8 {
                lanes[index] += chunk[index];
            }
        }
        let mut total = ((lanes[0] + lanes[1]) + (lanes[2] + lanes[3]))
            + ((lanes[4] + lanes[5]) + (lanes[6] + lanes[7]));
        for value in &values[limit..] {
            total += value;
        }
        return total;
    }
    let half = count / 2;
    let split = half - half % 8;
    sum(&values[..split]) + sum(&values[split..])
}

pub(super) fn peak(values: &[f64], index: i64) -> Result<f64> {
    let count = i64::try_from(values.len())?;
    if index == 0 || index == count - 1 {
        return Ok(index as f64);
    }
    let at = |index: i64| -> Result<f64> {
        let index = if index < 0 { count + index } else { index };
        Ok(*values
            .get(usize::try_from(index)?)
            .ok_or("peak index out of bounds")?)
    };
    let (previous, current, next) = (at(index - 1)?, at(index)?, at(index + 1)?);
    Ok(index as f64 + 0.5 * (next - previous) / (2. * current - next - previous))
}

pub(crate) fn smooth(values: &[f64], mask: &[f64], k: usize, sigma: f64) -> Result<Vec<f64>> {
    if k == 0 || k.is_multiple_of(2) {
        return Err("k must be positive and odd".into());
    }
    if values.is_empty() || values.len() != mask.len() {
        return Err("invalid smoothing shape".into());
    }
    let pad = k / 2;
    let mut weights: Vec<_> = (0..k)
        .map(|i| {
            let ratio = (i as f64 - pad as f64) / sigma;
            (-0.5 * ratio * ratio).exp()
        })
        .collect();
    let total = sum(&weights);
    for weight in &mut weights {
        *weight /= total;
    }
    let mut result = Vec::with_capacity(values.len());
    for index in 0..values.len() {
        let mut numerator = 0.;
        let mut denominator = 0.;
        for (offset, weight) in weights.iter().rev().enumerate() {
            let slot = index
                .saturating_add(offset)
                .saturating_sub(pad)
                .min(values.len() - 1);
            numerator += values[slot] * weight;
            denominator += mask[slot] * weight;
        }
        result.push(if denominator != 0. {
            numerator / denominator
        } else {
            f64::NAN
        });
    }
    Ok(result)
}

struct Fourier {
    transform: Transform,
    count: usize,
}
impl Fourier {
    fn new(count: usize) -> Result<Self> {
        Ok(Self {
            transform: Transform::new(count)?,
            count,
        })
    }
    fn forward(&mut self, values: &[f64]) -> Result<Vec<Complex>> {
        let mut data = vec![Complex::default(); self.count];
        for (value, target) in values.iter().zip(&mut data) {
            target.re = *value;
        }
        self.transform.execute(&mut data, 1., true)?;
        Ok(data)
    }
    fn product(&mut self, left: &[Complex], right: &[Complex]) -> Result<Vec<f64>> {
        // Exact NumPy 2.5.3 simd_cmul_f64 operand order, documented by the
        // PocketFFT boundary notice (AVX2/FMA3 and aarch64 NEON source dispatch).
        let mut data: Vec<_> = left
            .iter()
            .zip(right)
            .map(|(a, b)| Complex {
                re: a.re.mul_add(b.re, -(a.im * b.im)),
                im: a.re.mul_add(b.im, a.im * b.re),
            })
            .collect();
        self.transform
            .execute(&mut data, 1. / self.count as f64, false)?;
        Ok(data.into_iter().map(|value| value.re).collect())
    }
}

pub(super) fn correlate(
    expected: &[f64],
    actual: &[f64],
    mask: &[f64],
    count: usize,
) -> Result<Vec<f64>> {
    let rotated: Vec<_> = expected.iter().rev().copied().collect();
    let rotated_mask: Vec<_> = mask.iter().rev().copied().collect();
    let mut fft = Fourier::new(count)?;
    let actual_fft = fft.forward(actual)?;
    let expected_fft = fft.forward(&rotated)?;
    let mask_fft = fft.forward(mask)?;
    let rotated_mask_fft = fft.forward(&rotated_mask)?;
    let overlap: Vec<_> = fft
        .product(&rotated_mask_fft, &mask_fft)?
        .into_iter()
        .map(|value| value.round_ties_even().max(f64::EPSILON))
        .collect();
    let correlated_actual = fft.product(&rotated_mask_fft, &actual_fft)?;
    let correlated_expected = fft.product(&mask_fft, &expected_fft)?;
    let mut numerator = fft.product(&expected_fft, &actual_fft)?;
    for index in 0..count {
        numerator[index] -= correlated_actual[index] * correlated_expected[index] / overlap[index];
    }
    let actual_squared: Vec<_> = actual.iter().map(|value| value * value).collect();
    let actual_squared_fft = fft.forward(&actual_squared)?;
    let mut actual_denom = fft.product(&rotated_mask_fft, &actual_squared_fft)?;
    for index in 0..count {
        actual_denom[index] = (actual_denom[index]
            - correlated_actual[index] * correlated_actual[index] / overlap[index])
            .max(0.);
    }
    let expected_squared: Vec<_> = rotated.iter().map(|value| value * value).collect();
    let expected_squared_fft = fft.forward(&expected_squared)?;
    let mut expected_denom = fft.product(&mask_fft, &expected_squared_fft)?;
    for index in 0..count {
        expected_denom[index] = (expected_denom[index]
            - correlated_expected[index] * correlated_expected[index] / overlap[index])
            .max(0.);
    }
    let denom: Vec<_> = actual_denom
        .iter()
        .zip(expected_denom)
        .map(|(actual, expected)| (actual * expected).sqrt())
        .collect();
    let max = denom
        .iter()
        .map(|value| value.abs())
        .reduce(|a, b| {
            if a.is_nan() || b.is_nan() {
                f64::NAN
            } else {
                a.max(b)
            }
        })
        .ok_or("zero-size denominator reduction")?;
    let tolerance = 1e3 * f64::EPSILON * max;
    Ok((0..count)
        .map(|index| {
            if denom[index] > tolerance {
                (numerator[index] / denom[index]).clamp(-1., 1.)
            } else {
                0.
            }
        })
        .collect())
}

fn rounded_samples(seconds: f64, dt: f64) -> Result<i64> {
    if dt == 0. {
        return Err("float division by zero".into());
    }
    let value = (seconds / dt).round_ties_even();
    if !value.is_finite() || value < i64::MIN as f64 || value >= -(i64::MIN as f64) {
        return Err("invalid lag sample count".into());
    }
    Ok(value as i64)
}
fn sliced(values: &[f64], start: i64, stop: i64) -> Result<&[f64]> {
    let count = i64::try_from(values.len())?;
    let bound = |value: i64| {
        if value < 0 {
            (count + value).max(0)
        } else {
            value.min(count)
        }
    };
    let start = usize::try_from(bound(start))?;
    let stop = usize::try_from(bound(stop))?;
    Ok(&values[start..stop.max(start)])
}
pub(crate) fn delay(
    expected: &[f64],
    actual: &[f64],
    mask: &[f64],
    dt: f64,
    min: f64,
    max: f64,
) -> Result<Value> {
    if expected.len() != actual.len() {
        return Err("expected/actual lengths differ".into());
    }
    let (minimum, maximum, one_second) = (
        rounded_samples(min, dt)?,
        rounded_samples(max, dt)?,
        rounded_samples(1., dt)?,
    );
    let padded_input = i64::try_from(expected.len())?
        .checked_add(maximum.max(one_second))
        .ok_or("lag padding overflow")?;
    let size: usize = good_size(BigInt::from(padded_input)).to_string().parse()?;
    let ncc = correlate(expected, actual, mask, size)?;
    let base = i64::try_from(expected.len())? - 1;
    let region = sliced(
        &ncc,
        base.checked_add(minimum).ok_or("lag slice overflow")?,
        base.checked_add(maximum).ok_or("lag slice overflow")?,
    )?;
    let threshold = sliced(
        &ncc,
        base,
        base.checked_add(one_second).ok_or("lag slice overflow")?,
    )?;
    let confidence_region = sliced(
        &ncc,
        base.checked_sub(5).ok_or("lag slice overflow")?,
        base.checked_add(one_second)
            .and_then(|v| v.checked_add(5))
            .ok_or("lag slice overflow")?,
    )?;
    let mut maximum_index = 0;
    let first = *region
        .first()
        .ok_or("attempt to get argmax of an empty sequence")?;
    let mut correlation = first;
    for (index, value) in region.iter().copied().enumerate().skip(1) {
        if !correlation.is_nan() && (value.is_nan() || value > correlation) {
            maximum_index = index;
            correlation = value;
        }
    }
    let lag = peak(region, i64::try_from(maximum_index)?)? * dt + min;
    let maximum = threshold
        .iter()
        .copied()
        .reduce(|a, b| {
            if a.is_nan() || b.is_nan() {
                f64::NAN
            } else {
                a.max(b)
            }
        })
        .ok_or("zero-size threshold reduction")?;
    let minimum = threshold
        .iter()
        .copied()
        .reduce(|a, b| {
            if a.is_nan() || b.is_nan() {
                f64::NAN
            } else {
                a.min(b)
            }
        })
        .ok_or("zero-size threshold reduction")?;
    let threshold = (maximum - minimum) * 0.9 + minimum;
    let candidates: Vec<_> = confidence_region
        .iter()
        .map(|value| *value >= threshold)
        .collect();
    let mut starts = Vec::new();
    let mut ends = Vec::new();
    let mut previous = false;
    for (index, current) in candidates
        .iter()
        .copied()
        .chain(std::iter::once(false))
        .enumerate()
    {
        if current && !previous {
            starts.push(index);
        }
        if previous && !current {
            ends.push(index - 1);
        }
        previous = current;
    }
    if starts.is_empty() {
        return Err("candidate run index out of bounds".into());
    }
    let insertion = starts.partition_point(|start| *start <= maximum_index + 5);
    let run = if insertion == 0 {
        starts.len() - 1
    } else {
        insertion - 1
    };
    let width = ends[run] - starts[run] + 1;
    let confidence = (1. - width as f64 * dt).clamp(0., 1.);
    Ok(
        json!({"delay":normalized(lag),"correlation":normalized(correlation),"confidence":normalized(confidence),"peak":maximum_index,"run":run,"width":width,"starts":starts,"ends":ends}),
    )
}
