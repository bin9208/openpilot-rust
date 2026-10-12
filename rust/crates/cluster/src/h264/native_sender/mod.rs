mod worker;
pub use worker::Sender;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct AccessUnit {
    pub chunks: Vec<Vec<u8>>,
    pub codec_config: bool,
    pub keyframe: bool,
}
#[derive(Debug, serde::Serialize)]
pub struct Selection {
    pub kept: Vec<AccessUnit>,
    pub dropped_units: usize,
    pub dropped_chunks: usize,
}
#[must_use]
pub fn select_access_units(mut pending: Vec<AccessUnit>, incoming: AccessUnit) -> Selection {
    pending.push(incoming);
    let mut indexes = std::collections::BTreeSet::from([pending.len() - 1]);
    for property in [
        |unit: &AccessUnit| unit.codec_config,
        |unit: &AccessUnit| unit.keyframe,
    ] {
        if let Some(index) = pending.iter().rposition(property) {
            indexes.insert(index);
        }
    }
    let dropped_units = pending.len() - indexes.len();
    let dropped_chunks = pending
        .iter()
        .enumerate()
        .filter(|(index, _)| !indexes.contains(index))
        .map(|(_, unit)| unit.chunks.len())
        .sum();
    Selection {
        kept: pending
            .into_iter()
            .enumerate()
            .filter_map(|(index, unit)| indexes.contains(&index).then_some(unit))
            .collect(),
        dropped_units,
        dropped_chunks,
    }
}
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Snapshot {
    pub bytes_sent: usize,
    pub chunks_sent: usize,
    pub dropped_units: usize,
    pub dropped_chunks: usize,
    pub error: Option<String>,
}
