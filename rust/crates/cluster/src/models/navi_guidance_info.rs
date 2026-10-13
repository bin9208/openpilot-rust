use super::NaviItemMeta;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub struct NaviGuidanceInfo {
    pub meta: NaviItemMeta,
    pub distance_m: i64,
    pub time_s: i64,
    pub turn_type: i64,
    pub road_name: String,
    pub main_text: String,
    pub near_direction: String,
    pub mid_direction: String,
    pub far_direction: String,
}
