use crate::{sum::pairwise, Error};
use num_traits::ToPrimitive;
#[derive(Debug, serde::Serialize)]
pub struct BlockAverage {
    pub values: Vec<f64>,
    pub block_idx: usize,
    pub idx: usize,
    pub valid_blocks: i32,
    size: usize,
}
#[derive(Debug, serde::Serialize)]
pub struct Statistics {
    pub valid_mean: f64,
    pub valid_std: f64,
    pub current_mean: f64,
    pub current_std: f64,
}
fn statistics(values: &[f64]) -> Result<(f64, f64), Error> {
    if values.is_empty() {
        return Ok((f64::NAN, f64::NAN));
    }
    let size = values
        .len()
        .to_f64()
        .ok_or(Error::Contract("block count conversion"))?;
    let mean = pairwise(values) / size;
    let squares: Vec<_> = values.iter().map(|value| (value - mean).powi(2)).collect();
    Ok((mean, (pairwise(&squares) / size).sqrt()))
}
impl BlockAverage {
    pub fn new(count: usize, size: usize, seed: (f64, i32)) -> Result<Self, Error> {
        let count_int = i32::try_from(count).map_err(|_| Error::Contract("block count"))?;
        if count_int <= 0 || size == 0 {
            return Err(Error::Contract("positive block dimensions required"));
        }
        let block_idx = usize::try_from(seed.1.rem_euclid(count_int))
            .map_err(|_| Error::Contract("block index"))?;
        Ok(Self {
            values: vec![seed.0; count],
            block_idx,
            idx: 0,
            valid_blocks: seed.1,
            size,
        })
    }
    pub fn update(&mut self, value: f64) -> Result<(), Error> {
        let index = self
            .idx
            .to_f64()
            .ok_or(Error::Contract("block position conversion"))?;
        self.values[self.block_idx] = (index * self.values[self.block_idx] + value) / (index + 1.);
        self.idx = (self.idx + 1) % self.size;
        if self.idx == 0 {
            self.block_idx = (self.block_idx + 1) % self.values.len();
            self.valid_blocks = self
                .valid_blocks
                .saturating_add(1)
                .min(i32::try_from(self.values.len()).map_err(|_| Error::Contract("block count"))?);
        }
        Ok(())
    }
    pub fn statistics(&self) -> Result<Statistics, Error> {
        let mut valid = Vec::new();
        for index in 0..self.valid_blocks {
            let index = usize::try_from(index).map_err(|_| Error::Contract("valid block index"))?;
            if index != self.block_idx {
                valid.push(
                    *self
                        .values
                        .get(index)
                        .ok_or(Error::Contract("valid blocks exceed block count"))?,
                );
            }
        }
        let (valid_mean, valid_std) = statistics(&valid)?;
        if self.idx > 0 {
            valid.push(self.values[self.block_idx]);
        }
        let (current_mean, current_std) = statistics(&valid)?;
        Ok(Statistics {
            valid_mean,
            valid_std,
            current_mean,
            current_std,
        })
    }
}
