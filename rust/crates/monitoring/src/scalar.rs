//! Python min/max keep their first argument when comparison is unordered.
//! f64::min/max instead discard NaN, changing policy confidence and recovery.
pub(crate) fn min(first: f64, second: f64) -> f64 {
    if second < first {
        second
    } else {
        first
    }
}
pub(crate) fn max(first: f64, second: f64) -> f64 {
    if second > first {
        second
    } else {
        first
    }
}
pub(crate) fn clamp(value: f64, low: f64, high: f64) -> f64 {
    min(max(value, low), high)
}
