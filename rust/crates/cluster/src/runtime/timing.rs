#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct Timing {
    pub start: f64,
    pub last: f64,
    pub actual_fps: Option<f64>,
}
impl Timing {
    #[must_use]
    pub const fn new(start: f64) -> Self {
        Self {
            start,
            last: start,
            actual_fps: None,
        }
    }
    pub fn step(&mut self, now: f64) -> f64 {
        let dt = (now - self.last).max(0.001);
        self.last = now;
        if dt < 1.0 {
            self.actual_fps = Some(
                self.actual_fps
                    .map_or(1.0 / dt, |fps| fps * 0.85 + 1.0 / dt * 0.15),
            );
        }
        dt
    }
    #[must_use]
    pub fn finished(&self, now: f64, duration: Option<f64>) -> bool {
        duration.is_some_and(|duration| now - self.start >= duration)
    }
}
