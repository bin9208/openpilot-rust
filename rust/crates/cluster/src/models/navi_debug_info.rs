use super::{NaviGuidanceImage, NaviTrafficLightInfo};
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NaviDebugInfo {
    pub title: String,
    pub lines: Vec<String>,
    pub severity: String,
    pub speed_limit_kph: Option<i64>,
    pub traffic_light: Option<NaviTrafficLightInfo>,
    pub guidance_image: Option<NaviGuidanceImage>,
}
impl Default for NaviDebugInfo {
    fn default() -> Self {
        Self {
            title: String::new(),
            lines: Vec::new(),
            severity: "normal".into(),
            speed_limit_kph: None,
            traffic_light: None,
            guidance_image: None,
        }
    }
}
