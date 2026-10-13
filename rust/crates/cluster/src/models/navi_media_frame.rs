#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, Default)]
pub struct NaviMediaFrame {
    pub key: String,
    pub sequence: u64,
    pub present: bool,
    pub mime: String,
    pub width: i64,
    pub height: i64,
    pub data: Option<std::sync::Arc<[u8]>>,
    pub reason: Option<String>,
    pub plane_data: Option<[std::sync::Arc<[u8]>; 3]>,
    pub plane_strides: Option<[i64; 3]>,
    #[serde(default, with = "crate::media::frame::optional")]
    pub hardware_buffer: Option<std::sync::Arc<crate::media::HardwareFrame>>,
}
