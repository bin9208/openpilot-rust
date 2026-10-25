//! Admission at the original NumPy operations, separate from float64 casts.
use super::{array, integer, normalized, number, numbers, output};
use crate::seal::Result;
use num_bigint::BigInt;
use openpilot_log_format::{JsonValue, JsonView};
use serde_json::Value;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Dtype {
    Bool,
    Int,
    Float,
    Text,
    Object,
}
pub(super) fn dtype(values: &[JsonValue]) -> Dtype {
    let mut kind = if values.is_empty() {
        Dtype::Float
    } else {
        Dtype::Bool
    };
    for value in values {
        let next = match value.view() {
            JsonView::Bool(_) => Dtype::Bool,
            JsonView::Integer(_) => Dtype::Int,
            JsonView::Float(_) => Dtype::Float,
            JsonView::Text(_) => Dtype::Text,
            _ => Dtype::Object,
        };
        kind = if kind == Dtype::Object || next == Dtype::Object {
            Dtype::Object
        } else if kind == Dtype::Text || next == Dtype::Text {
            Dtype::Text
        } else if kind == Dtype::Float || next == Dtype::Float {
            Dtype::Float
        } else if kind == Dtype::Int || next == Dtype::Int {
            Dtype::Int
        } else {
            Dtype::Bool
        };
    }
    kind
}
pub(super) fn arithmetic(input: &JsonValue) -> Result<f64> {
    match input.view() {
        JsonView::Bool(_) | JsonView::Integer(_) | JsonView::Float(_) => number(input),
        _ => Err("TypeError: arithmetic requires a numeric scalar".into()),
    }
}
pub(super) fn arithmetic_array(input: &JsonValue) -> Result<(Vec<f64>, Dtype)> {
    let rows = array(input)?;
    let kind = dtype(&rows);
    if matches!(kind, Dtype::Text | Dtype::Object) {
        return Err("TypeError: ndarray arithmetic has no numeric loop".into());
    }
    Ok((rows.iter().map(arithmetic).collect::<Result<_>>()?, kind))
}
pub(super) fn padding(value: &JsonValue) -> Result<JsonValue> {
    match value.view() {
        JsonView::Bool(_) => Ok(value.clone()),
        JsonView::Integer(_) => Ok(JsonValue::parse(
            &super::numeric::good_size(integer(value)?).to_string(),
        )?),
        JsonView::Float(number) => {
            if number <= 6. || number.is_nan() {
                return Ok(value.clone());
            }
            if !number.is_finite() {
                return Err("nonfinite FFT size".into());
            }
            let ceiling: BigInt = format!("{:.0}", number.ceil()).parse()?;
            Ok(JsonValue::parse(
                &super::numeric::good_size(ceiling).to_string(),
            )?)
        }
        _ => Err("TypeError: FFT size comparison".into()),
    }
}
pub(super) fn peak(values: &JsonValue, index: &JsonValue) -> Result<JsonValue> {
    let rows = array(values)?;
    // Python equality precedes ndarray indexing and arithmetic, so a float or
    // bool boundary index is returned as-is, even for a string-valued ndarray.
    if matches!(
        index.view(),
        JsonView::Bool(_) | JsonView::Integer(_) | JsonView::Float(_)
    ) {
        let at = arithmetic(index)?;
        if at == 0. || at == rows.len() as f64 - 1. {
            return Ok(index.clone());
        }
    }
    let lane = PeakLane::infer(&rows)?;
    if matches!(index.view(), JsonView::Bool(true)) {
        // R[True] adds an axis; the two neighbouring indices are Python ints.
        let previous = rows.first().ok_or("peak index out of bounds")?;
        let next = rows.get(2).ok_or("peak index out of bounds")?;
        let offsets = rows
            .iter()
            .map(|current| Ok(normalized(1. + lane.offset(previous, current, next)?)))
            .collect::<Result<Vec<_>>>()?;
        return output(Value::Array(vec![Value::Array(offsets)]));
    }
    let at: i64 = integer(index)?.to_string().parse()?;
    let get = |position: i64| -> Result<&JsonValue> {
        let position = if position < 0 {
            i64::try_from(rows.len())? + position
        } else {
            position
        };
        rows.get(usize::try_from(position)?)
            .ok_or_else(|| "peak index out of bounds".into())
    };
    output(normalized(
        at as f64
            + lane.offset(
                get(at.checked_sub(1).ok_or("peak index overflow")?)?,
                get(at)?,
                get(at.checked_add(1).ok_or("peak index overflow")?)?,
            )?,
    ))
}

#[derive(Clone, Copy)]
enum PeakLane {
    Signed,
    Unsigned,
    Float,
    ObjectNumber,
}
impl PeakLane {
    fn infer(rows: &[JsonValue]) -> Result<Self> {
        let (mut signed, mut unsigned, mut float, mut object) = (false, false, false, false);
        for row in rows {
            match row.view() {
                JsonView::Bool(_) => {}
                JsonView::Integer(text) => {
                    if text.parse::<i64>().is_ok() {
                        signed = true;
                    } else if text.parse::<u64>().is_ok() {
                        unsigned = true;
                    } else {
                        object = true;
                    }
                }
                JsonView::Float(_) => float = true,
                _ => return Err("TypeError: ndarray peak arithmetic".into()),
            }
        }
        // NumPy discovers Python int as int64 or uint64 before common-dtype
        // promotion. Mixing these two produces f64, even for a positive small
        // int. Bool promotes directly to the other numeric dtype.
        if object {
            Ok(Self::ObjectNumber)
        } else if float || (signed && unsigned) {
            Ok(Self::Float)
        } else if unsigned {
            Ok(Self::Unsigned)
        } else if signed {
            Ok(Self::Signed)
        } else {
            Err("TypeError: numpy boolean subtraction".into())
        }
    }
    fn offset(self, previous: &JsonValue, current: &JsonValue, next: &JsonValue) -> Result<f64> {
        let (numerator, denominator) = match self {
            Self::Signed => {
                let previous: i64 = integer(previous)?.to_string().parse()?;
                let current: i64 = integer(current)?.to_string().parse()?;
                let next: i64 = integer(next)?.to_string().parse()?;
                (
                    next.wrapping_sub(previous) as f64,
                    current
                        .wrapping_mul(2)
                        .wrapping_sub(next)
                        .wrapping_sub(previous) as f64,
                )
            }
            Self::Unsigned => {
                let previous: u64 = integer(previous)?.to_string().parse()?;
                let current: u64 = integer(current)?.to_string().parse()?;
                let next: u64 = integer(next)?.to_string().parse()?;
                (
                    next.wrapping_sub(previous) as f64,
                    current
                        .wrapping_mul(2)
                        .wrapping_sub(next)
                        .wrapping_sub(previous) as f64,
                )
            }
            Self::Float => {
                let (previous, current, next) = (
                    arithmetic(previous)?,
                    arithmetic(current)?,
                    arithmetic(next)?,
                );
                (next - previous, 2. * current - next - previous)
            }
            Self::ObjectNumber => {
                // Object arrays retain Python ints/floats. Only actual float
                // operations convert arbitrary integers; do not pre-cast them.
                let (previous, current, next) = (
                    ObjectNumber::from_json(previous)?,
                    ObjectNumber::from_json(current)?,
                    ObjectNumber::from_json(next)?,
                );
                let numerator = next.subtract(&previous)?.float()?;
                let denominator = current
                    .twice()
                    .subtract(&next)?
                    .subtract(&previous)?
                    .float()?;
                if denominator == 0. {
                    return Err("float division by zero".into());
                }
                (numerator, denominator)
            }
        };
        Ok(0.5 * numerator / denominator)
    }
}

#[derive(Clone)]
enum ObjectNumber {
    Integer(BigInt),
    Float(f64),
}
impl ObjectNumber {
    fn from_json(value: &JsonValue) -> Result<Self> {
        match value.view() {
            JsonView::Integer(_) | JsonView::Bool(_) => Ok(Self::Integer(integer(value)?)),
            JsonView::Float(value) => Ok(Self::Float(value)),
            _ => Err("TypeError: object ndarray arithmetic".into()),
        }
    }
    fn float(&self) -> Result<f64> {
        match self {
            Self::Float(value) => Ok(*value),
            Self::Integer(value) => {
                let number: f64 = value.to_string().parse()?;
                if !number.is_finite() {
                    return Err("int too large to convert to float".into());
                }
                Ok(number)
            }
        }
    }
    fn twice(self) -> Self {
        match self {
            Self::Integer(value) => Self::Integer(value * 2u8),
            Self::Float(value) => Self::Float(value * 2.),
        }
    }
    fn subtract(&self, other: &Self) -> Result<Self> {
        Ok(match (self, other) {
            (Self::Integer(a), Self::Integer(b)) => Self::Integer(a - b),
            _ => Self::Float(self.float()? - other.float()?),
        })
    }
}

pub(super) fn smoothing(values: &JsonValue, mask: &JsonValue) -> Result<(Vec<f64>, Vec<f64>)> {
    let (values, _) = arithmetic_array(values)?;
    let (mask, _) = arithmetic_array(mask)?;
    let count = values.len().max(mask.len());
    if values.is_empty()
        || mask.is_empty()
        || (values.len() != count && values.len() != 1)
        || (mask.len() != count && mask.len() != 1)
    {
        return Err("operands could not be broadcast together".into());
    }
    let mask = (0..count).map(|i| mask[i % mask.len()]).collect::<Vec<_>>();
    let product = (0..count)
        .map(|i| values[i % values.len()] * mask[i])
        .collect();
    Ok((product, mask))
}
pub(super) fn correlation(
    expected: &JsonValue,
    actual: &JsonValue,
    mask: &JsonValue,
) -> Result<(Vec<f64>, Vec<f64>, Vec<f64>)> {
    let mut expected = numbers(expected)?;
    let mut actual = numbers(actual)?;
    let mask = array(mask)?;
    match dtype(&mask) {
        Dtype::Bool => {
            if expected.len() != mask.len() || actual.len() != mask.len() {
                return Err("boolean index did not match indexed array".into());
            }
            for (i, row) in mask.iter().enumerate() {
                if matches!(row.view(), JsonView::Bool(false)) {
                    expected[i] = 0.;
                    actual[i] = 0.;
                }
            }
        }
        Dtype::Int => {
            // ~integer_mask is an integer index array, not a validity bitmap.
            for row in &mask {
                let inverse = !integer(row)?;
                for values in [&mut expected, &mut actual] {
                    let index = if inverse < BigInt::from(0) {
                        BigInt::from(values.len()) + &inverse
                    } else {
                        inverse.clone()
                    };
                    let index: usize = index.to_string().parse()?;
                    *values
                        .get_mut(index)
                        .ok_or("integer mask index out of bounds")? = 0.;
                }
            }
        }
        _ => return Err("TypeError: invert/index requires boolean or integer ndarray".into()),
    }
    Ok((
        expected,
        actual,
        mask.iter().map(number).collect::<Result<_>>()?,
    ))
}
