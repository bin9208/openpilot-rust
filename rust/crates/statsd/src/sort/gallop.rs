//! CPython 3.12 galloping comparisons. NaN makes comparison order observable.
pub(super) fn left(key: f64, values: &[f64], hint: usize) -> usize {
    search(values, hint, |value| value < key)
}
pub(super) fn right(key: f64, values: &[f64], hint: usize) -> usize {
    search(values, hint, |value| {
        key.partial_cmp(&value) != Some(std::cmp::Ordering::Less)
    })
}
fn search(values: &[f64], hint: usize, precedes: impl Fn(f64) -> bool) -> usize {
    let mut last = 0;
    let mut offset = 1;
    let (mut low, mut high);
    if precedes(values[hint]) {
        let maximum = values.len() - hint;
        while offset < maximum && precedes(values[hint + offset]) {
            last = offset;
            offset = offset.saturating_mul(2).saturating_add(1);
        }
        low = hint + last + 1;
        high = hint + offset.min(maximum);
    } else {
        let maximum = hint + 1;
        while offset < maximum && !precedes(values[hint - offset]) {
            last = offset;
            offset = offset.saturating_mul(2).saturating_add(1);
        }
        low = maximum - offset.min(maximum);
        high = hint - last;
    }
    while low < high {
        let middle = low + (high - low) / 2;
        if precedes(values[middle]) {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    high
}
