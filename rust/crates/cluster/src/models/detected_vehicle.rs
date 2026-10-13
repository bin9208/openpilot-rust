#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DetectedVehicle {
    pub label: String,
    pub longitudinal_m: f64,
    pub lateral_m: f64,
    pub source: String,
    pub probability: f64,
    pub relative_speed_mps: Option<f64>,
    pub absolute_speed_kph: Option<f64>,
    pub lateral_speed_mps: Option<f64>,
    pub acceleration_mps2: Option<f64>,
    pub cut_in: bool,
    pub primary: bool,
    pub ttc_s: Option<f64>,
    pub x_std_m: Option<f64>,
    pub y_std_m: Option<f64>,
    pub radar_track_id: Option<i64>,
}
impl Default for DetectedVehicle {
    fn default() -> Self {
        Self {
            label: String::new(),
            longitudinal_m: 0.0,
            lateral_m: 0.0,
            source: "route".into(),
            probability: 1.0,
            relative_speed_mps: None,
            absolute_speed_kph: None,
            lateral_speed_mps: None,
            acceleration_mps2: None,
            cut_in: false,
            primary: false,
            ttc_s: None,
            x_std_m: None,
            y_std_m: None,
            radar_track_id: None,
        }
    }
}
