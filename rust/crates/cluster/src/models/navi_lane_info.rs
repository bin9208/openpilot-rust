use super::NaviItemMeta;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub struct NaviLaneInfo {
    pub meta: NaviItemMeta,
    pub count: i64,
    pub distance_m: i64,
    pub visible: bool,
    pub lane_play: bool,
    pub current_lane: i64,
    pub turn_code: i64,
    pub turn_info: Vec<i64>,
    pub available: Vec<i64>,
}
