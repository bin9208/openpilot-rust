use num_traits::ToPrimitive;

#[derive(Clone, Copy)]
pub(super) struct Stamp {
    pub mono: f64,
    pub wall: f64,
}
pub(super) fn now() -> Stamp {
    let mono = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    let wall = chrono::Utc::now();
    Stamp {
        mono: mono.tv_sec.to_f64().unwrap_or(0.0) + mono.tv_nsec.to_f64().unwrap_or(0.0) / 1e9,
        wall: wall.timestamp().to_f64().unwrap_or(0.0)
            + f64::from(wall.timestamp_subsec_nanos()) / 1e9,
    }
}
pub(super) fn round(value: f64, digits: usize) -> f64 {
    format!("{value:.digits$}").parse().unwrap_or(value)
}
