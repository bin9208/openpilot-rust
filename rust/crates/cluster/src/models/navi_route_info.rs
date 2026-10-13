use super::NaviItemMeta;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub struct NaviRouteInfo {
    pub meta: NaviItemMeta,
    pub remaining_distance_m: i64,
    pub remaining_time_s: i64,
    pub moved_distance_m: i64,
    pub moved_time_s: i64,
    pub total_distance_m: i64,
    pub polyline: Vec<[f64; 2]>,
}
