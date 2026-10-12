mod detections;
mod leads;
mod tracks;
use super::Data;
use crate::models::{DetectedVehicle, RadarPoint};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Radar {
    pub front_only: bool,
    pub brand: String,
    pub supported: bool,
    pub corner_seen: bool,
    pub can_detections: [Vec<DetectedVehicle>; 2],
    pub can_time: [f64; 2],
    detections: Vec<DetectedVehicle>,
    detection_time: f64,
    held_cutins: Vec<DetectedVehicle>,
    cutin_ids: BTreeSet<i64>,
    cutin_time: f64,
    sound_time: f64,
    points: BTreeMap<String, RadarPoint>,
    point_time: f64,
    pub debug_text: String,
}
impl Default for Radar {
    fn default() -> Self {
        Self {
            front_only: std::env::var("CLUSTER_ROUTE_FRONT_RADAR_ONLY")
                .is_ok_and(|value| value == "1"),
            brand: String::new(),
            supported: false,
            corner_seen: false,
            can_detections: [Vec::new(), Vec::new()],
            can_time: [-999.0; 2],
            detections: Vec::new(),
            detection_time: -999.0,
            held_cutins: Vec::new(),
            cutin_ids: BTreeSet::new(),
            cutin_time: -999.0,
            sound_time: -999.0,
            points: BTreeMap::new(),
            point_time: -999.0,
            debug_text: String::new(),
        }
    }
}
impl Radar {
    pub fn active(&self) -> bool {
        !self.front_only && (self.supported || self.corner_seen)
    }
    pub fn car_params(&mut self, params: Data<'_>) {
        if let Some(brand) = params.field("brand").text() {
            self.brand = brand.to_lowercase();
        }
        if !self.front_only
            && params
                .field("extFlags")
                .uint()
                .is_some_and(|flags| flags & ((1 << 12) | (1 << 13)) != 0)
        {
            self.supported = true;
        }
    }
    pub fn sound(&mut self, drive: Data<'_>, time: f64) {
        let sound = drive
            .field("alertSound")
            .text()
            .unwrap_or_default()
            .to_lowercase();
        let kind = drive
            .field("alertType")
            .text()
            .unwrap_or_default()
            .to_lowercase();
        if !self.cutin_ids.is_empty()
            && ((sound == "prompt" && kind.starts_with("audioprompt/"))
                || (sound == "radarcutin" && kind.starts_with("radarcutin/")))
        {
            self.sound_time = time;
        }
    }
    pub fn recorded(&self, time: f64) -> (bool, bool) {
        (
            time - self.cutin_time <= 0.08,
            time - self.sound_time <= 0.08,
        )
    }
}
fn corner_id(id: Option<i64>) -> bool {
    id.is_some_and(|id| {
        (200..220).contains(&id)
            || (240..250).contains(&id)
            || (300..412).contains(&id)
            || id >= 1000
    })
}
