use crate::Error;

pub(super) fn paired(
    left: &[f64],
    right: &[f64],
    operation: impl Fn(f64, f64) -> f64,
) -> Result<Vec<f64>, Error> {
    let length = if left.len() == right.len() {
        left.len()
    } else if left.len() == 1 {
        right.len()
    } else if right.len() == 1 {
        left.len()
    } else {
        return Err(Error::Contract("lane vector broadcasting dimensions"));
    };
    (0..length)
        .map(|index| Ok(operation(value(left, index)?, value(right, index)?)))
        .collect()
}

pub(super) fn value(values: &[f64], index: usize) -> Result<f64, Error> {
    let index = if values.len() == 1 { 0 } else { index };
    values
        .get(index)
        .copied()
        .ok_or(Error::Contract("lane vector index"))
}
