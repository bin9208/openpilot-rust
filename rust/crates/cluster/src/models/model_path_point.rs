#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub struct ModelPathPoint {
    pub forward_m: f64,
    pub lateral_m: f64,
    pub lateral_std_m: Option<f64>,
    pub speed_mps: Option<f64>,
    pub accel_mps2: Option<f64>,
    pub orientation_rad: Option<f64>,
    pub orientation_rate_rps: Option<f64>,
}
