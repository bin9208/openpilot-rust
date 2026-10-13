use super::NaviItemMeta;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub struct NaviVehicleInfo {
    pub meta: NaviItemMeta,
    pub latitude: f64,
    pub longitude: f64,
    pub heading_deg: f64,
    pub speed_kph: f64,
    pub road_name: String,
    pub virtual_gps: bool,
}
