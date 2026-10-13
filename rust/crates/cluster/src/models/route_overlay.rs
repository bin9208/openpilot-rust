#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RouteOverlay {
    pub video_rgba: Option<Vec<u8>>,
    pub video_width: i64,
    pub video_height: i64,
    pub video_frame_id: Option<String>,
    pub video_status: Option<String>,
    pub camera_stream: String,
    pub panel_visible: bool,
    pub cutin_status: Option<String>,
    pub data_lines: Vec<String>,
}
impl Default for RouteOverlay {
    fn default() -> Self {
        Self {
            video_rgba: None,
            video_width: 0,
            video_height: 0,
            video_frame_id: None,
            video_status: None,
            camera_stream: "road".into(),
            panel_visible: true,
            cutin_status: None,
            data_lines: Vec::new(),
        }
    }
}
