use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("missing model output {0}")]
    Missing(String),
    #[error("invalid model output slice {0}")]
    Slice(String),
    #[error("model output {name} has {actual} values, expected {expected}")]
    Width {
        name: String,
        actual: usize,
        expected: usize,
    },
}

pub struct RawOutputs<'a> {
    values: &'a [f32],
    slices: &'a BTreeMap<String, [usize; 2]>,
}

impl<'a> RawOutputs<'a> {
    pub fn new(
        values: &'a [f32],
        slices: &'a BTreeMap<String, [usize; 2]>,
    ) -> Result<Self, ParseError> {
        for (name, &[start, end]) in slices {
            if values.get(start..end).is_none() {
                return Err(ParseError::Slice(name.clone()));
            }
        }
        Ok(Self { values, slices })
    }

    pub fn get(&self, name: &str) -> Result<&[f32], ParseError> {
        let &[start, end] = self
            .slices
            .get(name)
            .ok_or_else(|| ParseError::Missing(name.to_owned()))?;
        Ok(&self.values[start..end])
    }

    pub fn has(&self, name: &str) -> bool {
        self.slices.contains_key(name)
    }

    pub fn array<const N: usize>(&self, name: &str) -> Result<[f32; N], ParseError> {
        let slice = self.get(name)?;
        slice.try_into().map_err(|_| ParseError::Width {
            name: name.to_owned(),
            actual: slice.len(),
            expected: N,
        })
    }

    pub fn normal<const N: usize>(&self, name: &str) -> Result<Normal<N>, ParseError> {
        let slice = self.get(name)?;
        if slice.len() != 2 * N {
            return Err(ParseError::Width {
                name: name.to_owned(),
                actual: slice.len(),
                expected: 2 * N,
            });
        }
        Ok(Normal {
            mean: std::array::from_fn(|i| slice[i]),
            std: std::array::from_fn(|i| safe_exp(slice[N + i])),
        })
    }

    pub fn leads(&self) -> Result<Normal<72>, ParseError> {
        let slice = self.get("lead")?;
        if slice.len() == 144 {
            return self.normal("lead");
        }
        let values = self.array::<102>("lead")?;
        let mut result = Normal {
            mean: [0.0; 72],
            std: [0.0; 72],
        };
        for selection in 0..3 {
            let mut weights = [values[48 + selection], values[99 + selection]];
            softmax(&mut weights);
            let winner = usize::from(weights[1].is_nan() || weights[1] >= weights[0]);
            for i in 0..24 {
                result.mean[selection * 24 + i] = values[winner * 51 + i];
                result.std[selection * 24 + i] = safe_exp(values[winner * 51 + 24 + i]);
            }
        }
        Ok(result)
    }
}

#[derive(Debug)]
pub struct Normal<const N: usize> {
    pub mean: [f32; N],
    pub std: [f32; N],
}

pub fn safe_exp(value: f32) -> f32 {
    crate::numpy_exp::exp(if value > 11.0 { 11.0 } else { value })
}

pub fn sigmoid(value: f32) -> f32 {
    1.0 / (1.0 + safe_exp(-value))
}

pub fn softmax(values: &mut [f32]) {
    let max = values.iter().copied().fold(f32::NEG_INFINITY, |a, b| {
        if a.is_nan() || b.is_nan() {
            f32::NAN
        } else {
            a.max(b)
        }
    });
    for value in values.iter_mut() {
        *value = safe_exp(*value - max);
    }
    let sum: f32 = values.iter().sum();
    for value in values {
        *value /= sum;
    }
}
