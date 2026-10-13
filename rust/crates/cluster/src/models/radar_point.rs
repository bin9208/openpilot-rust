#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub struct RadarPoint {
    pub label: String,
    pub longitudinal_m: f64,
    pub lateral_m: f64,
    pub source: String,
    pub relative_speed_mps: Option<f64>,
    pub absolute_speed_kph: Option<f64>,
    pub lateral_speed_mps: Option<f64>,
    pub relative_accel_mps2: Option<f64>,
    pub probability: Option<f64>,
    pub valid: Option<i64>,
    pub valid_count: Option<i64>,
    pub in_my_lane: Option<i64>,
    pub motion_consistent: Option<bool>,
    pub promotion_held: bool,
}
