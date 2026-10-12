#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub struct NaviItemStatus {
    pub key: String,
    pub sequence: u64,
    pub present: bool,
    pub reason: Option<String>,
}
