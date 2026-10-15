//! Source stopping_params.py lower bound; callers own blocking/nonblocking repair.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StoppingSpeed {
    pub meters_per_second: f64,
    pub repair: Option<i32>,
}

pub fn speed(stored: f64) -> StoppingSpeed {
    let value = if stored.is_finite() {
        stored.max(10.)
    } else {
        50.
    };
    StoppingSpeed {
        meters_per_second: value * 0.01,
        repair: if value != stored {
            Some(value as i32)
        } else {
            None
        },
    }
}
