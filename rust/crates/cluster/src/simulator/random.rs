use super::clamp;
use crate::models::SimulatorInput;
#[derive(Clone, Debug, serde::Serialize)]
pub struct RandomInputSource {
    elapsed: f64,
    next_signal_at: f64,
    next_signal_left: bool,
}
impl Default for RandomInputSource {
    fn default() -> Self {
        Self {
            elapsed: 0.0,
            next_signal_at: 4.0,
            next_signal_left: true,
        }
    }
}
impl RandomInputSource {
    pub fn update(&mut self, dt: f64) -> SimulatorInput {
        self.elapsed += dt;
        let mut throttle = 0.28 + 0.26 * (self.elapsed * 0.37).sin();
        let mut brake = 0.0;
        if (self.elapsed * 0.21).sin() < -0.72 {
            throttle = 0.0;
            brake = 0.34 + 0.28 * (self.elapsed * 0.78).sin().abs();
        }
        let mut left = false;
        let mut right = false;
        if self.elapsed >= self.next_signal_at {
            left = self.next_signal_left;
            right = !self.next_signal_left;
            self.next_signal_left = !self.next_signal_left;
            self.next_signal_at = self.elapsed + 8.0;
        }
        SimulatorInput {
            throttle: clamp(throttle, 0.0, 1.0),
            brake: clamp(brake, 0.0, 1.0),
            steering: 0.38 * (self.elapsed * 0.42).sin(),
            left_signal_requested: left,
            right_signal_requested: right,
            ..SimulatorInput::default()
        }
    }
}
