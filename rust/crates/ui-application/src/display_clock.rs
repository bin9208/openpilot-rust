//! Elapsed nominal 20 Hz frames for display-only animation and filters.
#[derive(Clone, Debug)]
pub struct DisplayClock {
    previous: Option<f64>,
    pub frames: f64,
}
impl Default for DisplayClock {
    fn default() -> Self {
        Self {
            previous: None,
            frames: 1.,
        }
    }
}
impl DisplayClock {
    pub fn tick(&mut self, now: f64) -> f64 {
        self.frames = self
            .previous
            .map_or(1., |previous| ((now - previous) * 20.).clamp(0., 5.));
        self.previous = Some(now);
        self.frames
    }
    pub fn retention(&self, value: f64) -> f64 {
        value.powf(self.frames)
    }
}
