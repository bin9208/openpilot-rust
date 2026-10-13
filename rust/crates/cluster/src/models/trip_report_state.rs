#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub struct TripReportState {
    pub duration_s: f64,
    pub moving_time_s: f64,
    pub distance_m: f64,
    pub average_speed_kph: f64,
    pub max_speed_kph: f64,
    pub max_accel_mps2: f64,
    pub max_decel_mps2: f64,
    pub auto_ratio_percent: f64,
    pub hard_accel_count: i64,
    pub hard_brake_count: i64,
    pub hard_corner_count: i64,
}
