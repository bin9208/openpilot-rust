use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct SyncData {
    pub timestamp: u64,
    pub frame_id_offset: u64,
    pub staggered: bool,
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct SyncTransition {
    pub synchronized: bool,
    pub timed_out: bool,
}

#[derive(Debug, Serialize)]
pub struct FrameSync {
    enabled_camera_count: usize,
    synced: bool,
    cameras: BTreeMap<i32, SyncData>,
    transition: SyncTransition,
}

impl FrameSync {
    pub fn new(enabled_camera_count: usize) -> Self {
        Self {
            enabled_camera_count,
            synced: false,
            cameras: BTreeMap::new(),
            transition: SyncTransition::default(),
        }
    }

    pub fn observe(&mut self, camera: i32, raw_id: u64, timestamp: u64, staggered: bool) -> bool {
        self.transition = SyncTransition::default();
        if self.synced {
            return true;
        }
        self.cameras.insert(
            camera,
            SyncData {
                timestamp,
                frame_id_offset: raw_id.wrapping_add(1),
                staggered,
            },
        );
        let aligned = self.cameras.iter().all(|(id, data)| {
            let expected = if staggered != data.staggered {
                25_000_000
            } else {
                0
            };
            *id == camera || timestamp.abs_diff(data.timestamp).abs_diff(expected) <= 200_000
        });
        if self.cameras.len() == self.enabled_camera_count && aligned {
            self.synced = true;
            self.transition.synchronized = true;
        }
        if raw_id > 40 {
            self.synced = true;
            self.transition.timed_out = true;
        }
        false
    }

    pub fn frame_id(&mut self, camera: i32, raw_id: u64) -> u32 {
        raw_id.wrapping_sub(self.cameras.entry(camera).or_default().frame_id_offset) as u32
    }

    pub fn is_synced(&self) -> bool {
        self.synced
    }

    pub fn enabled_camera_count(&self) -> usize {
        self.enabled_camera_count
    }

    pub fn cameras(&self) -> &BTreeMap<i32, SyncData> {
        &self.cameras
    }

    pub fn transition(&self) -> SyncTransition {
        self.transition
    }
}
