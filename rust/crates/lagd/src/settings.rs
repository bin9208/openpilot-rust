#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct Settings {
    pub dt: f64,
    pub block_count: usize,
    pub min_valid_block_count: usize,
    pub block_size: usize,
    pub window_sec: f64,
    pub okay_window_sec: f64,
    pub min_recovery_buffer_sec: f64,
    pub min_vego: f64,
    pub min_yr: f64,
    pub min_ncc: f64,
    pub max_lat_accel: f64,
    pub max_lat_accel_diff: f64,
    pub min_confidence: f64,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            dt: 0.05,
            block_count: 50,
            min_valid_block_count: 5,
            block_size: 100,
            window_sec: 60.,
            okay_window_sec: 25.,
            min_recovery_buffer_sec: 2.,
            min_vego: 15.,
            min_yr: 0.,
            min_ncc: 0.95,
            max_lat_accel: 2.,
            max_lat_accel_diff: 0.6,
            min_confidence: 0.7,
        }
    }
}
