//! Exact Bosch complete-link grouping configuration and elementary gates.
use super::{physical::VisionCue, tracking::Track};
use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    pub distance_diameter_m: f64,
    pub lateral_diameter_m: f64,
    pub velocity_diameter_mps: f64,
    pub min_pair_observations: usize,
    pub min_pair_span_s: f64,
    pub pair_max_gap_s: f64,
    pub evidence_window_s: f64,
    pub max_relative_distance_growth_m: f64,
    pub max_relative_lateral_growth_m: f64,
    pub stationary_speed_mps: f64,
    pub moving_speed_mps: f64,
    pub max_members: usize,
    pub coast_s: f64,
    pub first_physical_id: u32,
    pub provisional_enabled: bool,
    pub mature_retention_enabled: bool,
    pub provisional_min_distance_m: f64,
    pub provisional_max_distance_m: f64,
    pub provisional_max_lateral_m: f64,
    pub provisional_max_bearing_deg: f64,
    pub provisional_max_velocity_mps: f64,
    pub provisional_max_world_speed_mps: f64,
    pub provisional_min_range_m: f64,
    pub provisional_min_world_speed_mps: f64,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            distance_diameter_m: 3.0,
            lateral_diameter_m: 1.5,
            velocity_diameter_mps: 1.5,
            min_pair_observations: 3,
            min_pair_span_s: 0.18,
            pair_max_gap_s: 0.16,
            evidence_window_s: 0.8,
            max_relative_distance_growth_m: 1.0,
            max_relative_lateral_growth_m: 0.75,
            stationary_speed_mps: 0.6,
            moving_speed_mps: 1.4,
            max_members: 8,
            coast_s: 0.3,
            first_physical_id: 1000000,
            provisional_enabled: true,
            mature_retention_enabled: false,
            provisional_min_distance_m: 1.0,
            provisional_max_distance_m: 2.0,
            provisional_max_lateral_m: 0.25,
            provisional_max_bearing_deg: 0.2,
            provisional_max_velocity_mps: 0.25,
            provisional_max_world_speed_mps: 0.05,
            provisional_min_range_m: 40.0,
            provisional_min_world_speed_mps: 4.0,
        }
    }
}
impl Config {
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("distance_diameter_m", self.distance_diameter_m),
            ("lateral_diameter_m", self.lateral_diameter_m),
            ("velocity_diameter_mps", self.velocity_diameter_mps),
            ("min_pair_observations", self.min_pair_observations as f64),
            ("min_pair_span_s", self.min_pair_span_s),
            ("pair_max_gap_s", self.pair_max_gap_s),
            ("evidence_window_s", self.evidence_window_s),
            (
                "max_relative_distance_growth_m",
                self.max_relative_distance_growth_m,
            ),
            (
                "max_relative_lateral_growth_m",
                self.max_relative_lateral_growth_m,
            ),
            ("stationary_speed_mps", self.stationary_speed_mps),
            ("moving_speed_mps", self.moving_speed_mps),
            ("max_members", self.max_members as f64),
            ("coast_s", self.coast_s),
            ("first_physical_id", self.first_physical_id as f64),
            (
                "provisional_min_distance_m",
                self.provisional_min_distance_m,
            ),
            (
                "provisional_max_distance_m",
                self.provisional_max_distance_m,
            ),
            ("provisional_max_lateral_m", self.provisional_max_lateral_m),
            (
                "provisional_max_bearing_deg",
                self.provisional_max_bearing_deg,
            ),
            (
                "provisional_max_velocity_mps",
                self.provisional_max_velocity_mps,
            ),
            (
                "provisional_max_world_speed_mps",
                self.provisional_max_world_speed_mps,
            ),
            ("provisional_min_range_m", self.provisional_min_range_m),
            (
                "provisional_min_world_speed_mps",
                self.provisional_min_world_speed_mps,
            ),
        ] {
            if !value.is_finite() || value <= 0. {
                return Err(format!("{name} must be positive and finite"));
            }
        }
        if self.min_pair_observations < 2 {
            return Err("at least two integer observations are required".into());
        }
        if !(2..=32).contains(&self.max_members) {
            return Err("max_members must be an integer in [2,32]".into());
        }
        if self.stationary_speed_mps >= self.moving_speed_mps {
            return Err("kinematic stationary and moving bands must be disjoint".into());
        }
        if self.min_pair_span_s > self.evidence_window_s {
            return Err("evidence window must contain the minimum pair span".into());
        }
        if self.first_physical_id >= 1 << 31 {
            return Err("physical IDs must fit positive downstream Int32".into());
        }
        if self.provisional_min_distance_m >= self.provisional_max_distance_m {
            return Err("provisional distance band must be nonempty".into());
        }
        if self.provisional_max_distance_m > self.distance_diameter_m {
            return Err("provisional pairs must remain inside normal grouping geometry".into());
        }
        Ok(())
    }
    pub fn geometry(&self, a: &Track, b: &Track, v_ego: f64) -> (bool, &'static str) {
        let (a, b) = (&a.detection, &b.detection);
        if (a.d_rel - b.d_rel).abs() > self.distance_diameter_m {
            return (false, "distance_diameter");
        }
        if (a.y_rel - b.y_rel).abs() > self.lateral_diameter_m {
            return (false, "lateral_diameter");
        }
        if (a.v_rel - b.v_rel).abs() > self.velocity_diameter_mps {
            return (false, "velocity_diameter");
        }
        if v_ego.is_finite() {
            let (a, b) = ((a.v_rel + v_ego).abs(), (b.v_rel + v_ego).abs());
            if a.min(b) <= self.stationary_speed_mps && a.max(b) >= self.moving_speed_mps {
                return (false, "stationary_moving_conflict");
            }
        }
        (true, "geometry")
    }
}
pub fn vision_support(member: &Track, vision: &[VisionCue]) -> bool {
    let member = &member.detection;
    vision.iter().any(|c| {
        c.d_rel.is_finite()
            && c.y_rel.is_finite()
            && c.probability >= 0.7
            && (member.d_rel - c.d_rel).abs() <= c.distance_tolerance_m
            && (member.y_rel - c.y_rel).abs() <= c.lateral_tolerance_m
    })
}
