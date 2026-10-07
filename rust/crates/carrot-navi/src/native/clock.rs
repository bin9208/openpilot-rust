use crate::record::Clock;
use rustix::time::{clock_gettime, ClockId};

pub struct NativeClock;

pub fn seconds() -> f64 {
    python_seconds(ClockId::Monotonic)
}

fn python_seconds(id: ClockId) -> f64 {
    let time = clock_gettime(id);
    let nanos = i128::from(time.tv_sec) * 1_000_000_000 + i128::from(time.tv_nsec);
    if nanos % 1_000_000_000 == 0 {
        (nanos / 1_000_000_000) as f64
    } else {
        nanos as f64 / 1e9
    }
}

pub fn nanos(id: ClockId) -> u128 {
    let time = clock_gettime(id);
    u128::try_from(time.tv_sec).unwrap_or_default() * 1_000_000_000
        + u128::try_from(time.tv_nsec).unwrap_or_default()
}

pub(super) fn wall_millis(time: rustix::time::Timespec) -> i128 {
    let nanos = i128::from(time.tv_sec) * 1_000_000_000 + i128::from(time.tv_nsec);
    nanos.div_euclid(1_000_000)
}

impl Clock for NativeClock {
    fn wall_ms(&mut self) -> i128 {
        wall_millis(clock_gettime(ClockId::Realtime))
    }
    fn mono_ns(&mut self) -> u128 {
        nanos(ClockId::Monotonic)
    }
}
