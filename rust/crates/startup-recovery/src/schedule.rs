//! Source recovery display checks once after five seconds, then every thirty.
pub struct Schedule {
    next_check: f64,
    was_busy: bool,
}
impl Schedule {
    pub fn new(now: f64) -> Self {
        Self {
            next_check: now + 5.0,
            was_busy: false,
        }
    }
    pub fn poll(&mut self, busy: bool, now: impl FnMut() -> f64) -> bool {
        self.poll_with(busy, now, || Ok::<(), std::convert::Infallible>(()))
            .expect("infallible schedule action")
    }
    pub fn poll_with<E>(
        &mut self,
        busy: bool,
        mut now: impl FnMut() -> f64,
        action: impl FnOnce() -> Result<(), E>,
    ) -> Result<bool, E> {
        if self.was_busy && !busy {
            self.next_check = now() + 30.0;
        }
        self.was_busy = busy;
        if !busy && now() >= self.next_check {
            action()?;
            self.next_check = now() + 30.0;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
