//! Ordered raw association followed by physical grouping; publication is separate.
use super::{
    group, group_config,
    physical::{Object, VisionCue},
    raw::RawDetection,
    tracking,
};
use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
pub struct Tracker {
    pub raw_manager: tracking::Manager,
    pub group_manager: group::Manager,
    pub last_raw_tracks: Vec<tracking::Track>,
}
impl Tracker {
    pub fn new(raw: tracking::Config, group: group_config::Config) -> Result<Self, String> {
        Ok(Self {
            raw_manager: tracking::Manager::new(raw, 1)?,
            group_manager: group::Manager::new(group)?,
            last_raw_tracks: vec![],
        })
    }
    pub fn set_decision_trace(&mut self, enabled: bool) {
        self.raw_manager.trace_decisions = enabled;
        self.group_manager.trace_decisions = enabled;
    }
    pub fn update(
        &mut self,
        timestamp: i64,
        detections: &[RawDetection],
        yaw: Option<f64>,
        ego: f64,
        oem: Option<usize>,
        vision: &[VisionCue],
    ) -> Result<Vec<Object>, String> {
        let raw = self.raw_manager.update(timestamp, detections, yaw)?;
        self.last_raw_tracks = raw;
        self.group_manager
            .update(timestamp, &self.last_raw_tracks, yaw, ego, oem, vision)
            .map_err(str::to_owned)
    }
}
