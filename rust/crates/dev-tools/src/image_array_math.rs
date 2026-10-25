//! Raw integer image-array comparison compatible with the original NumPy gate.
use crate::seal::Result;

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct PixelDifference {
    pub different_pixels: usize,
    pub max_channel_difference: i64,
}

fn elements(shape: &[usize]) -> Result<usize> {
    if shape.len() > 64 {
        return Err("array exceeds the original 64-dimension limit".into());
    }
    shape.iter().try_fold(1usize, |count, &dimension| {
        count
            .checked_mul(dimension)
            .ok_or_else(|| "array shape overflows address space".into())
    })
}

fn strides(shape: &[usize], rank: usize, broadcast: bool) -> Vec<usize> {
    let mut result = vec![0; rank];
    let mut stride = 1;
    for (axis, &dimension) in shape.iter().enumerate().rev() {
        result[rank - shape.len() + axis] = if broadcast && dimension == 1 {
            0
        } else {
            stride
        };
        stride *= dimension;
    }
    result
}

/// Arrays are dense row-major signed 64-bit samples, as produced by the original
/// Pillow/NumPy `.astype(int)` path on its supported Linux architectures. Preserve
/// NumPy's wrapping subtraction/absolute value, including `abs(i64::MIN) == MIN`.
/// No broadcasted image or difference array is allocated.
pub fn compare(
    left_shape: &[usize],
    left: &[i64],
    right_shape: &[usize],
    right: &[i64],
) -> Result<PixelDifference> {
    if elements(left_shape)? != left.len() || elements(right_shape)? != right.len() {
        return Err("image samples do not match their array shape".into());
    }
    let rank = left_shape.len().max(right_shape.len());
    let mut shape = vec![1; rank];
    for (axis, dimension) in shape.iter_mut().enumerate() {
        let value = |input: &[usize]| {
            axis.checked_sub(rank - input.len())
                .map_or(1, |index| input[index])
        };
        let (a, b) = (value(left_shape), value(right_shape));
        *dimension = if a == b || b == 1 {
            a
        } else if a == 1 {
            b
        } else {
            return Err("image array dimensions cannot be broadcast together".into());
        };
    }
    let count = elements(&shape)?;
    if count == 0 {
        return Err("zero-size array has no maximum channel difference".into());
    }
    let output_strides = strides(&shape, rank, false);
    let left_strides = strides(left_shape, rank, true);
    let right_strides = strides(right_shape, rank, true);
    let channels = shape.last().copied().unwrap_or(1);
    let mut any = false;
    let mut result = PixelDifference {
        different_pixels: 0,
        max_channel_difference: i64::MIN,
    };
    for position in 0..count {
        let (mut a, mut b) = (0, 0);
        for axis in 0..rank {
            let coordinate = position / output_strides[axis] % shape[axis];
            a += coordinate * left_strides[axis];
            b += coordinate * right_strides[axis];
        }
        let difference = left[a].wrapping_sub(right[b]).wrapping_abs();
        result.max_channel_difference = result.max_channel_difference.max(difference);
        any |= difference != 0;
        if position % channels == channels - 1 {
            result.different_pixels += usize::from(any);
            any = false;
        }
    }
    Ok(result)
}
