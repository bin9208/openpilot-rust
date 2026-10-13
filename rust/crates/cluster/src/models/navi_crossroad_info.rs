use super::NaviItemMeta;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub struct NaviCrossroadInfo {
    pub meta: NaviItemMeta,
    pub visible: bool,
    pub distance_m: i64,
    pub image_code: i64,
}
