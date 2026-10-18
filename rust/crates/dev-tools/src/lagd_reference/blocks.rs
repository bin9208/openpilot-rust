//! Source BlockAverage arithmetic, including ndarray assignment dtype.
use super::{
    admission::{arithmetic, dtype, Dtype},
    normalized_array,
    numeric::sum,
};
use crate::seal::Result;
use openpilot_log_format::{JsonValue, JsonView};
use serde_json::json;

pub(super) struct Blocks {
    values: Vec<JsonValue>,
    dtype: Dtype,
    block_size: JsonValue,
    block_idx: usize,
    idx: f64,
    floating_idx: bool,
    valid_blocks: i64,
}
impl Blocks {
    pub fn new(count: usize, size: JsonValue, valid: i64, initial: JsonValue) -> Result<Self> {
        if count == 0 {
            return Err("integer modulo by zero".into());
        }
        let count_signed = i64::try_from(count)?;
        let kind = dtype(std::slice::from_ref(&initial));
        Ok(Self {
            values: vec![initial; count],
            dtype: kind,
            block_size: size,
            block_idx: usize::try_from(valid.rem_euclid(count_signed))?,
            idx: 0.,
            floating_idx: false,
            valid_blocks: valid,
        })
    }
    pub fn update(&mut self, value: f64) -> Result<()> {
        let denominator = self.idx + 1.;
        let previous = arithmetic(&self.values[self.block_idx])?;
        let updated = (self.idx * previous + value) / denominator;
        // np.tile keeps the initial dtype. Assignment casts the floating result
        // back into that storage; integer blocks must not silently become f64.
        self.values[self.block_idx] = match self.dtype {
            Dtype::Bool => JsonValue::parse(if updated != 0. { "true" } else { "false" })?,
            Dtype::Int => {
                let integer = if updated.is_finite()
                    && updated >= i64::MIN as f64
                    && updated < -(i64::MIN as f64)
                {
                    updated.trunc() as i64
                } else {
                    i64::MIN
                };
                JsonValue::parse(&integer.to_string())?
            }
            Dtype::Float => JsonValue::parse(&if updated.is_nan() {
                "NaN".into()
            } else if updated == f64::INFINITY {
                "Infinity".into()
            } else if updated == f64::NEG_INFINITY {
                "-Infinity".into()
            } else {
                format!("{updated:?}")
            })?,
            _ => return Err("TypeError: block ndarray arithmetic".into()),
        };
        let size = arithmetic(&self.block_size)?;
        if size == 0. {
            return Err("integer modulo by zero".into());
        }
        // Python modulo follows the divisor's sign (including negative sizes),
        // and a floating divisor makes subsequent idx values Python floats.
        self.idx = denominator % size;
        if self.idx != 0. && self.idx.is_sign_negative() != size.is_sign_negative() {
            self.idx += size;
        }
        if self.idx == 0. {
            self.idx = 0f64.copysign(size);
        }
        self.floating_idx = matches!(self.block_size.view(), JsonView::Float(_));
        if self.idx == 0. {
            self.block_idx = (self.block_idx + 1) % self.values.len();
            self.valid_blocks = (self.valid_blocks + 1).min(i64::try_from(self.values.len())?);
        }
        Ok(())
    }
    fn statistics(&self) -> Result<[f64; 4]> {
        let count = usize::try_from(self.valid_blocks.max(0))?;
        let mut valid = Vec::new();
        for index in 0..count {
            if index != self.block_idx {
                valid.push(arithmetic(
                    self.values.get(index).ok_or("block index out of bounds")?,
                )?);
            }
        }
        fn stats(values: &[f64]) -> (f64, f64) {
            if values.is_empty() {
                return (f64::NAN, f64::NAN);
            }
            let mean = sum(values) / values.len() as f64;
            let differences: Vec<_> = values
                .iter()
                .map(|value| {
                    let delta = value - mean;
                    delta * delta
                })
                .collect();
            (mean, (sum(&differences) / values.len() as f64).sqrt())
        }
        let (valid_mean, valid_std) = stats(&valid);
        if self.idx > 0. {
            valid.push(arithmetic(&self.values[self.block_idx])?);
        }
        let (current_mean, current_std) = stats(&valid);
        Ok([valid_mean, valid_std, current_mean, current_std])
    }
    pub fn snapshot(&self) -> Result<JsonValue> {
        let values = self
            .values
            .iter()
            .map(|value| match value.view() {
                JsonView::Float(value) => Ok(super::normalized(value).to_string()),
                _ => value.to_json(),
            })
            .collect::<std::result::Result<Vec<_>, _>>()?
            .join(",");
        let idx = if self.floating_idx {
            super::normalized(self.idx)
        } else {
            json!(self.idx as i64)
        };
        Ok(JsonValue::parse(&format!("{{\"values\":[{values}],\"block_idx\":{},\"idx\":{idx},\"valid_blocks\":{},\"statistics\":{}}}",self.block_idx,self.valid_blocks,normalized_array(&self.statistics()?)))?)
    }
}
