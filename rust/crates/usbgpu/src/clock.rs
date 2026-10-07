use std::time::{Duration, Instant};
pub trait Clock {
    fn now(&self) -> Duration;
    fn sleep(&mut self, duration: Duration);
}
pub struct WallClock {
    started: Instant,
}
impl Default for WallClock {
    fn default() -> Self {
        Self {
            started: Instant::now(),
        }
    }
}
impl Clock for WallClock {
    fn now(&self) -> Duration {
        self.started.elapsed()
    }
    fn sleep(&mut self, duration: Duration) {
        std::thread::sleep(duration);
    }
}
