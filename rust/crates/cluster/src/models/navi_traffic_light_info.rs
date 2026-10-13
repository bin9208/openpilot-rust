use super::NaviItemMeta;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub struct NaviTrafficLightInfo {
    pub distance_m: Option<i64>,
    pub red_s: Option<i64>,
    pub straight_s: Option<i64>,
    pub left_s: Option<i64>,
    pub right_s: Option<i64>,
    pub uturn_s: Option<i64>,
    pub red_on: Option<bool>,
    pub straight_on: Option<bool>,
    pub left_on: Option<bool>,
    pub right_on: Option<bool>,
    pub uturn_on: Option<bool>,
    pub meta: Option<NaviItemMeta>,
}
