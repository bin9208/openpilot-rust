#[derive(Debug)]
pub struct FanController {
    last_ignition: bool,
    c3: bool,
    integral: f64,
    rate: f64,
}
impl FanController {
    pub fn new(rate: u32, device: &str) -> Self {
        Self {
            last_ignition: false,
            c3: matches!(device, "tici" | "tizi"),
            integral: 0.,
            rate: f64::from(rate),
        }
    }
    pub fn update(&mut self, temperature: f64, ignition: bool) -> i32 {
        let (low, high) = if ignition { (30., 100.) } else { (0., 30.) };
        if ignition != self.last_ignition {
            self.integral = 0.;
        }
        self.last_ignition = ignition;
        let target = if self.c3 { 75. } else { 70. };
        let feedforward = if self.c3 {
            ((temperature - 60.) * 2.5).clamp(0., 100.)
        } else {
            0.
        };
        let control = feedforward.clamp(low, high);
        self.integral = (self.integral + (temperature - target) * 0.004 / self.rate)
            .clamp(low - control, high - control);
        // Source int truncates the bounded PID output, never rounds it.
        (feedforward + self.integral).clamp(low, high) as i32
    }
}
