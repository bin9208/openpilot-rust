//! NumPy compiled_base.c interpolation search, including duplicate and unsorted inputs.
use crate::Error;
fn search(key: f64, x: &[f64], previous: isize) -> isize {
    let len = x.len();
    if key > x[len - 1] {
        return len as isize;
    }
    if key < x[0] {
        return -1;
    }
    if len <= 4 {
        let mut i = 1;
        while i < len && key >= x[i] {
            i += 1;
        }
        return i as isize - 1;
    }
    let guess = previous.max(1).min(len as isize - 3) as usize;
    let (mut low, mut high) = (0, len);
    if key < x[guess] {
        if key < x[guess - 1] {
            high = guess - 1;
            if guess > 8 && key >= x[guess - 8] {
                low = guess - 8;
            }
        } else {
            return guess as isize - 1;
        }
    } else if key < x[guess + 1] {
        return guess as isize;
    } else if key < x[guess + 2] {
        return guess as isize + 1;
    } else {
        low = guess + 2;
        if guess + 9 < len && key < x[guess + 8] {
            high = guess + 8;
        }
    }
    while low < high {
        let mid = low + ((high - low) >> 1);
        if key >= x[mid] {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    low as isize - 1
}
pub fn interp(query: &[f64], x: &[f64], y: &[f64]) -> Result<Vec<f64>, Error> {
    if x.is_empty() || x.len() != y.len() {
        return Err(Error::Contract("interpolation history empty or mismatched"));
    }
    let mut previous = 0;
    Ok(query
        .iter()
        .map(|&value| {
            if x.len() == 1 {
                return y[0];
            }
            if value.is_nan() {
                return value;
            }
            previous = search(value, x, previous);
            if previous == -1 {
                return y[0];
            }
            let i = previous as usize;
            if i >= x.len() - 1 {
                return y[y.len() - 1];
            }
            if value == x[i] {
                return y[i];
            }
            let slope = (y[i + 1] - y[i]) / (x[i + 1] - x[i]);
            let mut result = slope * (value - x[i]) + y[i];
            if result.is_nan() {
                result = slope * (value - x[i + 1]) + y[i + 1];
                if result.is_nan() && y[i] == y[i + 1] {
                    result = y[i];
                }
            }
            result
        })
        .collect())
}
pub fn engagement_times(time: f64, lag: f64) -> Result<Vec<f64>, Error> {
    let start = time - 2.;
    let length = ((time + lag - start) / 0.05).ceil();
    if !length.is_finite() || length > isize::MAX as f64 {
        return Err(Error::Contract("invalid arange length"));
    }
    let length = length.max(0.) as usize;
    let mut result = Vec::new();
    result
        .try_reserve_exact(length)
        .map_err(|_| Error::Contract("arange allocation failed"))?;
    // NumPy fill uses the representable (start + step) - start increment.
    let delta = (start + 0.05) - start;
    for i in 0..length {
        result.push(if i == 0 {
            start
        } else if i == 1 {
            start + 0.05
        } else {
            start + i as f64 * delta
        });
    }
    Ok(result)
}
