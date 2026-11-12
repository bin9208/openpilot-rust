//! One immutable vehicle baseline, with coherent geometry and axle stiffness.
const GEOMETRY_RATE: f64 = 0.1; // Ratio per second; a one-percent adjustment takes 0.1 s.
const CLOSE: f64 = 1e-7;

/// CenterToFrontRatio percent when in range, otherwise the vehicle's own ratio.
pub fn requested_ratio(percent: i64, center_to_front: f64, wheelbase: f64) -> f64 {
    if (31..=69).contains(&percent) {
        percent as f64 * 0.01
    } else {
        center_to_front / wheelbase
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LateralGeometry {
    pub wheelbase: f64,
    default_ratio: f64,
    default_front: f64,
    default_rear: f64,
    pub ratio: f64,
    pub center_to_front: f64,
    pub stiffness_front: f64,
    pub stiffness_rear: f64,
}

impl LateralGeometry {
    pub fn new(wheelbase: f64, center_to_front: f64, front: f64, rear: f64) -> Self {
        let ratio = center_to_front / wheelbase;
        Self {
            wheelbase,
            default_ratio: ratio,
            default_front: front,
            default_rear: rear,
            ratio,
            center_to_front,
            stiffness_front: front,
            stiffness_rear: rear,
        }
    }

    fn admissible(&self, ratio: f64) -> bool {
        // Zero is an absent/legacy message, never a CG at the front axle.
        ratio.is_finite()
            && ((0.3 < ratio && ratio < 0.7) || (ratio - self.default_ratio).abs() <= CLOSE)
    }

    pub fn update(&mut self, ratio: f64) -> bool {
        if !self.admissible(ratio) || (ratio - self.ratio).abs() <= CLOSE {
            return false;
        }
        self.ratio = ratio;
        self.center_to_front = self.wheelbase * ratio;
        self.stiffness_front = self.default_front * (1. - ratio) / (1. - self.default_ratio);
        self.stiffness_rear = self.default_rear * ratio / self.default_ratio;
        true
    }

    pub fn step(&mut self, target: f64, dt: f64) -> bool {
        if !target.is_finite() || !dt.is_finite() || dt <= 0. || !self.admissible(target) {
            return false;
        }
        let limit = GEOMETRY_RATE * dt.min(0.05);
        self.update(self.ratio + (target - self.ratio).min(limit).max(-limit))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_absent_ratio_and_scales_stiffness() {
        let mut geometry = LateralGeometry::new(3., 1.5, 100., 200.);
        assert!(!geometry.update(0.));
        assert!(geometry.update(0.4));
        assert!((geometry.center_to_front - 1.2).abs() < 1e-9);
        assert!((geometry.stiffness_front - 120.).abs() < 1e-9);
        assert!((geometry.stiffness_rear - 160.).abs() < 1e-9);
    }

    #[test]
    fn step_is_rate_limited() {
        let mut geometry = LateralGeometry::new(3., 1.5, 100., 200.);
        assert!(geometry.step(0.4, 1.));
        assert!((geometry.ratio - 0.495).abs() < 1e-9);
        assert_eq!(requested_ratio(0, 1.5, 3.), 0.5);
        assert_eq!(requested_ratio(40, 1.5, 3.), 0.4);
    }
}
