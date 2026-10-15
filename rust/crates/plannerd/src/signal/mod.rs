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

/// Both MPC leads remain obstacles; only nearby leads veto extra preparation.
pub fn prepare_lead_blocked(radar: &crate::radar::Radar) -> bool {
    [radar.lead_one, radar.lead_two]
        .iter()
        .any(|lead| lead.status && (!lead.d_rel.is_finite() || lead.d_rel <= 50.))
}

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

#[cfg(test)]
mod near_lead_tests {
    #[test]
    fn both_leads_block_only_nearby_and_nonfinite_distances() {
        for index in 0..2 {
            for (distance, blocked) in [
                (-1., true),
                (4., true),
                (50., true),
                (50.01, false),
                (80., false),
                (f64::NAN, true),
                (f64::INFINITY, true),
                (f64::NEG_INFINITY, true),
            ] {
                let mut radar = crate::radar::Radar::default();
                let lead = if index == 0 {
                    &mut radar.lead_one
                } else {
                    &mut radar.lead_two
                };
                lead.status = true;
                lead.d_rel = distance;
                assert_eq!(super::prepare_lead_blocked(&radar), blocked);
                if index == 0 {
                    radar.lead_one.status = false;
                } else {
                    radar.lead_two.status = false;
                }
                assert!(!super::prepare_lead_blocked(&radar));
            }
        }
    }
}
