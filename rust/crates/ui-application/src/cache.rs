//! Last-complete Params snapshots and bounded retry/pending-write policy.
use serde::Serialize;
#[derive(Clone, Debug, Serialize)]
pub struct TimedCache<T> {
    pub value: T,
    pub next_refresh_time: f64,
    interval: f64,
    failures: u32,
    pending: Option<T>,
    pending_deadline: f64,
}
impl<T: Clone + PartialEq> TimedCache<T> {
    pub fn new(value: T) -> Self {
        Self::with_interval(value, 0.5)
    }
    pub fn with_interval(value: T, interval: f64) -> Self {
        Self {
            value,
            next_refresh_time: 0.0,
            interval,
            failures: 0,
            pending: None,
            pending_deadline: 0.0,
        }
    }
    pub fn refresh<E>(&mut self, now: f64, load: impl FnOnce() -> Result<T, E>) -> &T {
        if now < self.next_refresh_time {
            return &self.value;
        }
        let value = match load() {
            Ok(value) => value,
            Err(_) => {
                let delay = self
                    .interval
                    .min(0.05 * f64::from(1u32 << self.failures.min(4)));
                self.failures = self.failures.saturating_add(1);
                self.next_refresh_time = now + delay;
                return &self.value;
            }
        };
        self.failures = 0;
        if let Some(pending) = &self.pending {
            if &value != pending && now < self.pending_deadline {
                self.next_refresh_time = now + self.interval;
                return &self.value;
            }
            self.pending = None;
        }
        self.value = value;
        self.next_refresh_time = now + self.interval;
        &self.value
    }
    pub fn store_pending(&mut self, value: T, now: f64) {
        self.pending = Some(value.clone());
        self.pending_deadline = now + 1.0;
        self.failures = 0;
        self.value = value;
        self.next_refresh_time = now + self.interval;
    }
}
