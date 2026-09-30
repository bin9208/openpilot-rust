//! Deadline progression from common/realtime.py Ratekeeper(0.5).
use std::time::Duration;

const INTERVAL: Duration = Duration::from_secs(2);

#[derive(Default)]
pub struct Cadence {
    next: Option<Duration>,
}

impl Cadence {
    /// Called after publication, including when collection overruns its deadline.
    /// The source initializes its first deadline on the first keep_time call.
    pub fn deadline(&mut self, now: Duration) -> Duration {
        let deadline = self.next.unwrap_or(now + INTERVAL);
        self.next = Some(deadline + INTERVAL);
        deadline
    }
}
