//! Exact pinned ARS510 profile defaults; experimental profiles remain opt-in.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct NativeInterfaceConfig {
    pub radar_bus: i64,
    pub ego_speed_bus: i64,
    pub max_ego_speed_age_s: f64,
    pub include_metadata: bool,
    pub min_publish_age: i64,
    pub relink_max_gap_s: f64,
    pub vground_scale: f64,
    pub drop_unresolved_vrel: bool,
    pub range_fusion_gain: f64,
    pub drop_saturated_codes: bool,
    pub sat_recover_mps: f64,
    pub guard_hold_s: f64,
    pub guard_min_age: i64,
    pub acc_target_max_age_s: f64,
    pub acc_match_range_m: f64,
    pub acc_match_range_frac: f64,
    pub acc_range: bool,
    pub acc_match_min_age: i64,
    pub acc_sticky_jump_m: f64,
    pub acc_sticky_max_cost: f64,
    pub path_gate_m: f64,
    pub path_gate_min_range_m: f64,
    pub far_acc_only_m: f64,
    pub path_gate_stationary_mps: f64,
    pub path_gate_stationary_m: f64,
    pub summary_max_range_m: f64,
    pub summary_window_s: f64,
    pub summary_match_lat_m: f64,
    pub summary_scales: Vec<(f64, f64)>,
    pub fused_speed_filter: bool,
    pub speed_sigma_per_code: f64,
    pub young_sigma_scale: f64,
    pub young_age: i64,
    pub acc_sigma_mps: f64,
    pub summary_sigma_mps: f64,
    pub lead_accel_std_mps2: f64,
    pub innovation_gate_sigma: f64,
    pub publish_speed_std_mps: f64,
    pub speed_bias_tau_s: f64,
    pub speed_bias_sigma_per_code: f64,
    pub speed_sigma_ego_gain: f64,
    pub speed_sigma_accel_gain: f64,
    pub speed_init_sigma_per_code: f64,
    pub speed_divergence_inflation: bool,
}
impl Default for NativeInterfaceConfig {
    fn default() -> Self {
        Self {
            radar_bus: 1,
            ego_speed_bus: 0,
            max_ego_speed_age_s: 0.5,
            include_metadata: true,
            min_publish_age: 1,
            relink_max_gap_s: 0.0,
            vground_scale: 1.0,
            drop_unresolved_vrel: false,
            range_fusion_gain: 0.0,
            drop_saturated_codes: false,
            sat_recover_mps: 5.0,
            guard_hold_s: 1.0,
            guard_min_age: 60,
            acc_target_max_age_s: 0.1,
            acc_match_range_m: 12.0,
            acc_match_range_frac: 0.4,
            acc_range: true,
            acc_match_min_age: 20,
            acc_sticky_jump_m: 8.0,
            acc_sticky_max_cost: 4.0,
            path_gate_m: 0.0,
            path_gate_min_range_m: 15.0,
            far_acc_only_m: 0.0,
            path_gate_stationary_mps: 0.0,
            path_gate_stationary_m: 1.5,
            summary_max_range_m: 80.0,
            summary_window_s: 1.0,
            summary_match_lat_m: 1.0,
            summary_scales: vec![(0.0625, -10.0), (0.0625, -10.0)],
            fused_speed_filter: false,
            speed_sigma_per_code: 0.045,
            young_sigma_scale: 1.8,
            young_age: 100,
            acc_sigma_mps: 0.5,
            summary_sigma_mps: 0.5,
            lead_accel_std_mps2: 1.5,
            innovation_gate_sigma: 3.0,
            publish_speed_std_mps: 0.75,
            speed_bias_tau_s: 0.0,
            speed_bias_sigma_per_code: 0.0,
            speed_sigma_ego_gain: 0.0,
            speed_sigma_accel_gain: 0.0,
            speed_init_sigma_per_code: 0.0,
            speed_divergence_inflation: false,
        }
    }
}
impl NativeInterfaceConfig {
    pub fn all_tracks() -> Self {
        Self::default()
    }
    pub fn base() -> Self {
        Self {
            min_publish_age: 60,
            relink_max_gap_s: 3.5,
            vground_scale: 0.9933333333333333,
            drop_unresolved_vrel: true,
            drop_saturated_codes: true,
            ..Self::default()
        }
    }
    pub fn fused() -> Self {
        Self {
            relink_max_gap_s: 0.0,
            range_fusion_gain: 0.1,
            drop_saturated_codes: false,
            path_gate_m: 2.5,
            far_acc_only_m: 60.0,
            path_gate_stationary_mps: 0.5,
            fused_speed_filter: true,
            acc_sigma_mps: 0.13,
            summary_sigma_mps: 0.0,
            ..Self::base()
        }
    }
    pub fn colored() -> Self {
        Self {
            speed_sigma_per_code: 0.001,
            young_sigma_scale: 1.0,
            acc_sigma_mps: 0.5,
            summary_sigma_mps: 0.5,
            lead_accel_std_mps2: 1.0,
            speed_bias_tau_s: 1.19,
            speed_bias_sigma_per_code: 0.0096,
            speed_sigma_ego_gain: 0.48,
            speed_sigma_accel_gain: 0.51,
            speed_init_sigma_per_code: 0.045,
            speed_divergence_inflation: true,
            ..Self::fused()
        }
    }
}
