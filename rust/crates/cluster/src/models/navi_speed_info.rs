use super::NaviItemMeta;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub struct NaviSpeedInfo {
    pub meta: NaviItemMeta,
    pub current_kph: f64,
    pub road_limit_kph: Option<i64>,
    pub sdi_type: Option<i64>,
    pub sdi_distance_m: Option<i64>,
    pub sdi_speed_limit_kph: Option<i64>,
    pub secondary_sdi_type: Option<i64>,
    pub secondary_sdi_distance_m: Option<i64>,
    pub secondary_sdi_speed_limit_kph: Option<i64>,
    pub section_active: bool,
    pub section_speed_limit_kph: Option<i64>,
    pub section_average_kph: Option<f64>,
    pub section_remaining_distance_m: Option<f64>,
    pub section_remaining_time_s: Option<i64>,
    pub section_progress: Option<f64>,
}
