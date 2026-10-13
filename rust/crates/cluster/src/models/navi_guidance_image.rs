#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub struct NaviGuidanceImage {
    pub image_base64: String,
    pub image_mime: String,
    pub image_hash: String,
    pub width: i64,
    pub height: i64,
}
