use crate::Error;
use num_traits::ToPrimitive;

pub(crate) fn now() -> Result<f64, Error> {
    let time = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    let seconds = time
        .tv_sec
        .to_f64()
        .ok_or_else(|| Error::Source("monotonic seconds conversion".into()))?;
    let nanos = time
        .tv_nsec
        .to_f64()
        .ok_or_else(|| Error::Source("monotonic nanoseconds conversion".into()))?;
    Ok(seconds + nanos / 1e9)
}

pub(crate) enum Heartbeat {
    PingAt(f64),
    PongBefore(f64),
}

impl Heartbeat {
    pub fn new(now: f64) -> Self {
        Self::PingAt((now + 20.).ceil())
    }

    pub fn activity(&mut self, now: f64) {
        *self = Self::PingAt((now + 20.).ceil());
    }

    pub fn ping_sent(&mut self, now: f64) {
        *self = Self::PongBefore((now + 10.).ceil());
    }

    pub fn deadline(&self) -> f64 {
        match self {
            Self::PingAt(deadline) | Self::PongBefore(deadline) => *deadline,
        }
    }
}
