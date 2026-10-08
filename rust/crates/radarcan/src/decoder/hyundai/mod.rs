mod constructor;
pub mod group3;
pub mod identity;
pub use constructor::Environment;
mod corner;
mod front;
mod scc;
mod update;

use crate::{integer_set::IntegerSet, reader::Reader};

pub struct Hyundai {
    pub canfd: bool,
    pub radar_group1: bool,
    pub radar_group3: bool,
    pub radar_group4: bool,
    pub radar_start_addr: u32,
    pub radar_msg_count: u32,
    pub radar_required_msg_count: u32,
    pub radar_tracks: bool,
    pub corner_object_tracks: bool,
    pub corner_object_180_tracks: bool,
    pub corner_object_430_tracks: bool,
    pub rcp_tracks: Option<Reader>,
    pub rcp_scc: Option<Reader>,
    pub rcp_corner_objects: Option<Reader>,
    pub rcp_corner_objects_180: Option<Reader>,
    pub updated_tracks: IntegerSet,
    pub updated_scc: IntegerSet,
    pub updated_corner_objects: IntegerSet,
    pub updated_corner_objects_180: IntegerSet,
    pub updated_corner_objects_430: IntegerSet,
    pub corner_object_missed_updates: u64,
    pub corner_object_180_missed_updates: u64,
    pub corner_object_430_missed_updates: u64,
    pub corner_object_track_ids: identity::TrackIds,
    pub group3_track_ids: group3::TrackIds,
    pub trigger_msg_tracks: u32,
    pub trigger_msg_scc: u32,
    pub trigger_msg_corner_objects: u32,
    pub trigger_msg_corner_objects_180: u32,
    pub trigger_msg_corner_objects_430: u32,
    pub corner_objects_available: bool,
    pub radar_off_can: bool,
    pub track_id: u64,
    pub v_rel_last: f64,
    pub d_rel_last: f64,
}
