use super::{NaviItemStatus, NaviMediaFrame};
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct NaviDashboardState {
    pub connected: bool,
    pub endpoint: String,
    pub session_id: String,
    pub app_version: String,
    pub manifest_revision: i64,
    pub received_count: i64,
    pub last_received_age_ms: Option<i64>,
    pub map_frame_age_ms: Option<i64>,
    pub map_stream_stalled: bool,
    pub peer: String,
    pub error: Option<String>,
    pub media: Vec<NaviMediaFrame>,
    pub items: Vec<NaviItemStatus>,
    pub app_status: String,
    pub camera_status: String,
    pub composition_status: String,
}
impl Default for NaviDashboardState {
    fn default() -> Self {
        Self {
            connected: false,
            endpoint: String::new(),
            session_id: String::new(),
            app_version: String::new(),
            manifest_revision: 0,
            received_count: 0,
            last_received_age_ms: None,
            map_frame_age_ms: None,
            map_stream_stalled: false,
            peer: "-".into(),
            error: None,
            media: Vec::new(),
            items: Vec::new(),
            app_status: String::new(),
            camera_status: String::new(),
            composition_status: String::new(),
        }
    }
}
