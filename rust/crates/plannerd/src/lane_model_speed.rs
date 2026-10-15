//! Continuous usable model horizon gate, final source lane_model_speed.py.
#[derive(Debug)]
pub struct LaneModelSpeedGuard {
    recovery_frames: u64,
    valid_frames: u64,
}

impl LaneModelSpeedGuard {
    pub const fn new(recovery_frames: u64) -> Self {
        Self {
            recovery_frames,
            valid_frames: 0,
        }
    }

    pub fn update(&mut self, ego: f64, start: f64, end: f64) -> bool {
        let valid = [ego, start, end].iter().all(|v| v.is_finite() && *v >= 0.)
            && start >= ego * 0.7
            && end >= start * 0.7;
        self.valid_frames = if valid {
            self.valid_frames
                .saturating_add(1)
                .min(self.recovery_frames.saturating_add(1))
        } else {
            0
        };
        self.valid_frames > self.recovery_frames
    }
}
