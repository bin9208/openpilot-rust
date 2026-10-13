#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub struct DebugPlotSnapshot {
    pub mode: i64,
    pub title: String,
    pub values: [f64; 3],
}
