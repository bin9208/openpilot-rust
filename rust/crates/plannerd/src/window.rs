use crate::Error;
use num_traits::ToPrimitive;

#[derive(Clone, Debug)]
pub struct Window<const N: usize> {
    values: [f64; N],
    length: usize,
}

impl<const N: usize> Default for Window<N> {
    fn default() -> Self {
        const {
            assert!(N > 0);
        }
        Self {
            values: [0.; N],
            length: 0,
        }
    }
}

impl<const N: usize> Window<N> {
    pub fn clear(&mut self) {
        self.length = 0;
    }

    pub fn push(&mut self, value: f64) {
        if self.length == N {
            self.values.copy_within(1.., 0);
            self.values[N - 1] = value;
        } else {
            self.values[self.length] = value;
            self.length += 1;
        }
    }

    pub fn mean(&self) -> Result<f64, Error> {
        if self.length == 0 {
            return Err(Error::Contract("empty moving average"));
        }
        let mut total = 0_f64;
        let mut correction = 0_f64;
        for value in &self.values[..self.length] {
            let next = total + value;
            correction += if total.abs() >= value.abs() {
                (total - next) + value
            } else {
                (value - next) + total
            };
            total = next;
        }
        if correction != 0. && correction.is_finite() {
            total += correction;
        }
        Ok(total
            / self
                .length
                .to_f64()
                .ok_or(Error::Contract("window length conversion"))?)
    }

    pub fn median(&self) -> Result<f64, Error> {
        if self.length == 0 {
            return Err(Error::Contract("empty moving median"));
        }
        let mut values = self.values;
        let values = &mut values[..self.length];
        if values.iter().any(|value| value.is_nan()) {
            return Ok(f64::NAN);
        }
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let middle = self.length / 2;
        if self.length.is_multiple_of(2) {
            Ok((values[middle - 1] + values[middle]) / 2.)
        } else {
            Ok(values[middle])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Window;

    #[test]
    fn source_sum_retains_cancellation_residual() {
        // Given: CPython's moving-average input loses a unit with naive summation.
        let mut values = Window::<3>::default();
        for value in [1e16, 1., -1e16] {
            values.push(value);
        }
        // When: calculating the source mean.
        let mean = values.mean().unwrap();
        // Then: the compensated residual contributes to the mean.
        assert_eq!(mean, 1. / 3.);
    }

    #[test]
    fn eviction_changes_only_the_oldest_sample() {
        // Given: a full window followed by one replacement sample.
        let mut values = Window::<3>::default();
        for value in [100., 1., 2., 3.] {
            values.push(value);
        }
        // When: calculating the current median.
        let median = values.median().unwrap();
        // Then: the oldest outlier has been evicted.
        assert_eq!(median, 2.);
    }
}
