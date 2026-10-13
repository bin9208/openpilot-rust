#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub struct NaviItemMeta {
    pub sequence: u64,
    pub source_timestamp_ms: u64,
    pub received_mono_s: f64,
}
