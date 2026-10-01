use crate::Error;
use num_traits::ToPrimitive;
use openpilot_pocketfft::{Complex, Transform};
pub fn next_good_size(n: usize) -> Result<usize, Error> {
    if n <= 6 {
        return Ok(n);
    }
    let mut best = n
        .checked_mul(2)
        .ok_or(Error::Contract("FFT size overflow"))?;
    let mut a = 1;
    while a < best {
        let mut b = a;
        while b < best {
            let mut c = b;
            while c < best {
                let mut d = c;
                while d < best {
                    let mut e = d;
                    while e < best {
                        if e >= n {
                            best = e;
                        }
                        e = e
                            .checked_mul(11)
                            .ok_or(Error::Contract("FFT size overflow"))?;
                    }
                    d = d
                        .checked_mul(7)
                        .ok_or(Error::Contract("FFT size overflow"))?;
                }
                c = c
                    .checked_mul(5)
                    .ok_or(Error::Contract("FFT size overflow"))?;
            }
            b = b
                .checked_mul(3)
                .ok_or(Error::Contract("FFT size overflow"))?;
        }
        a = a
            .checked_mul(2)
            .ok_or(Error::Contract("FFT size overflow"))?;
    }
    Ok(best)
}
pub struct Correlator {
    n: usize,
    scale: f64,
    kernel: Transform,
}
impl Correlator {
    pub fn new(n: usize) -> Result<Self, Error> {
        if n == 0 {
            return Err(Error::Contract("FFT size zero"));
        }
        Ok(Self {
            n,
            scale: 1. / n.to_f64().ok_or(Error::Contract("FFT size conversion"))?,
            kernel: Transform::new(n)?,
        })
    }
    fn fft(&mut self, values: impl Iterator<Item = f64>) -> Result<Vec<Complex>, Error> {
        let mut buffer = vec![Complex::default(); self.n];
        for (slot, value) in buffer.iter_mut().zip(values) {
            slot.re = value;
        }
        self.kernel.execute(&mut buffer, 1., true)?;
        Ok(buffer)
    }
    fn convolve(&mut self, left: &[Complex], right: &[Complex]) -> Result<Vec<f64>, Error> {
        let mut buffer: Vec<_> = left
            .iter()
            .zip(right)
            .map(|(a, b)| Complex {
                re: a.re.mul_add(b.re, -(a.im * b.im)),
                im: a.re.mul_add(b.im, a.im * b.re),
            })
            .collect();
        self.kernel.execute(&mut buffer, self.scale, false)?;
        Ok(buffer.iter().map(|value| value.re).collect())
    }
    pub fn masked(
        &mut self,
        expected: &[f64],
        actual: &[f64],
        mask: &[bool],
    ) -> Result<Vec<f64>, Error> {
        if expected.len() != actual.len() || expected.len() != mask.len() {
            return Err(Error::Contract("correlation shape"));
        }
        let expected: Vec<_> = expected
            .iter()
            .zip(mask)
            .map(|(&value, &valid)| if valid { value } else { 0. })
            .collect();
        let actual: Vec<_> = actual
            .iter()
            .zip(mask)
            .map(|(&value, &valid)| if valid { value } else { 0. })
            .collect();
        let actual_fft = self.fft(actual.iter().copied())?;
        let expected_fft = self.fft(expected.iter().rev().copied())?;
        let mask_fft = self.fft(mask.iter().map(|value| f64::from(*value)))?;
        let reversed_mask_fft = self.fft(mask.iter().rev().map(|value| f64::from(*value)))?;
        let overlap: Vec<_> = self
            .convolve(&reversed_mask_fft, &mask_fft)?
            .into_iter()
            .map(|value| value.round_ties_even().max(f64::EPSILON))
            .collect();
        let ma = self.convolve(&reversed_mask_fft, &actual_fft)?;
        let me = self.convolve(&mask_fft, &expected_fft)?;
        let mut numerator = self.convolve(&expected_fft, &actual_fft)?;
        let actual_squared_fft = self.fft(actual.iter().map(|value| value * value))?;
        let expected_squared_fft = self.fft(expected.iter().rev().map(|value| value * value))?;
        let mut da = self.convolve(&reversed_mask_fft, &actual_squared_fft)?;
        let mut de = self.convolve(&mask_fft, &expected_squared_fft)?;
        let mut denominator = Vec::with_capacity(self.n);
        for index in 0..self.n {
            numerator[index] -= ma[index] * me[index] / overlap[index];
            da[index] = (da[index] - ma[index].powi(2) / overlap[index]).max(0.);
            de[index] = (de[index] - me[index].powi(2) / overlap[index]).max(0.);
            denominator.push((da[index] * de[index]).sqrt());
        }
        let tolerance = 1e3
            * f64::EPSILON
            * denominator
                .iter()
                .map(|value| value.abs())
                .fold(0., f64::max);
        Ok(numerator
            .iter()
            .zip(&denominator)
            .map(|(&num, &den)| {
                if den > tolerance {
                    (num / den).clamp(-1., 1.)
                } else {
                    0.
                }
            })
            .collect())
    }
}
