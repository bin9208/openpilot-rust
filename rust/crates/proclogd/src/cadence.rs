//! Final upstream procLog schedules each publication from its current loop time.
use std::time::Duration;
const INTERVAL: Duration = Duration::from_secs(2);
#[derive(Default)]
pub struct Cadence;
impl Cadence {
    /// Schedule from publication start; never replay slots missed during a stall.
    pub fn deadline(&mut self, now: Duration) -> Duration {
        now + INTERVAL
    }
}
