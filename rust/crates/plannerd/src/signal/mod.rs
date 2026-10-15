//! Final-source signal decisions; observations never grant acceleration authority.
#[cfg(feature = "native-skip-miri")]
pub mod context;
#[cfg(feature = "native-skip-miri")]
pub mod geometry;
pub mod longitudinal;
pub mod prepare;
pub mod priority;
pub mod road_policy;
pub mod start;
pub mod statistics;
pub mod tracker;
pub mod transport;

/// Source preparation cap; reset on every ineligible or non-finite sample.
#[derive(Debug, Default)]
pub struct PrepareAccel {
    cap: Option<f64>,
}
impl PrepareAccel {
    pub fn apply(&mut self, target: f64, previous: f64, dt: f64, active: bool) -> f64 {
        let eligible =
            active && [target, previous, dt].iter().all(|v| v.is_finite()) && 0. < dt && dt <= 0.2;
        if !eligible {
            self.cap = None;
            return target;
        }
        let cap = (self.cap.unwrap_or(previous.max(-0.5)) - 0.25 * dt).max(-0.5);
        self.cap = Some(cap);
        target.min(cap)
    }
}
