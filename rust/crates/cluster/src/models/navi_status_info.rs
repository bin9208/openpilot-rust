use super::NaviItemMeta;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub struct NaviStatusInfo {
    pub meta: NaviItemMeta,
    pub mode: String,
    pub guidance_active: bool,
    pub off_route: bool,
    pub route_present: bool,
}
