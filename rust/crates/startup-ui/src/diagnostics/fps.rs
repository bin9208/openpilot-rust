//! Original warning throttle and strict shutdown thresholds.
#[derive(Clone, Copy, Debug, serde::Deserialize)]
pub struct Sample {
    pub fps: i32,
    pub now: f64,
    pub strict: bool,
}
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct Decision {
    pub warning: bool,
    pub critical: bool,
    pub last_log: f64,
}
pub struct Monitor {
    target: i32,
    last_log: f64,
}
impl Monitor {
    pub fn new(target: i32, now: f64) -> Self {
        Self {
            target,
            last_log: now,
        }
    }
    pub fn observe(&mut self, sample: Sample) -> Decision {
        let warning = f64::from(sample.fps) < f64::from(self.target) * 0.9
            && sample.now - self.last_log >= 5.0;
        if warning {
            self.last_log = sample.now;
        }
        Decision {
            warning,
            critical: sample.strict && f64::from(sample.fps) < f64::from(self.target) * 0.5,
            last_log: self.last_log,
        }
    }
}
