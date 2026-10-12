use super::{corner_id, Data, Radar};
use crate::{
    live::projection::points::ttc,
    models::{radar_position_is_zero, DetectedVehicle},
};
use std::collections::BTreeSet;

impl Radar {
    pub fn update(&mut self, radar: Data<'_>, time: f64, speed: f64) {
        let cuts = radar.field("leadsCutIn").items();
        self.cutin_ids = cuts
            .iter()
            .filter(|lead| lead.flag("status", false))
            .filter_map(|lead| lead.field("radarTrackId").int())
            .collect();
        let mut detections = Vec::new();
        let mut primary = BTreeSet::new();
        for (label, field) in [("L1", "leadOne"), ("L2", "leadTwo")] {
            let lead = radar.field(field);
            let id = lead.field("radarTrackId").int();
            if self.front_only && self.brand == "hyundai" && corner_id(id) {
                continue;
            }
            let cut = field == "leadTwo" && id.is_some_and(|id| self.cutin_ids.contains(&id));
            if let Some(detection) =
                lead_detection(lead, if cut { "L2 CUT-IN" } else { label }, cut, speed)
            {
                if let Some(id) = id {
                    primary.insert(id);
                }
                detections.push(detection);
            }
        }
        for lead in &cuts {
            if lead
                .field("radarTrackId")
                .int()
                .is_some_and(|id| primary.contains(&id))
            {
                continue;
            }
            if let Some(detection) = lead_detection(*lead, "CUT-IN", true, speed) {
                detections.push(detection);
            }
        }
        if let Some(lead) = cuts.iter().find(|lead| lead.flag("status", false)) {
            let current: Vec<_> = detections
                .iter()
                .filter(|detection| detection.cut_in)
                .cloned()
                .collect();
            if !current.is_empty() {
                self.held_cutins = current;
            }
            self.cutin_time = time;
            self.debug_text = format!(
                "LOG CUT-IN: YES | id{} x {:.1}m y {:+.1}m",
                lead.field("radarTrackId").int().unwrap_or(-1),
                lead.number("dRel", 0.0),
                lead.number("yRel", 0.0)
            );
        } else if time - self.cutin_time <= 0.08 && !self.held_cutins.is_empty() {
            let held_ids: BTreeSet<_> = self
                .held_cutins
                .iter()
                .map(|detection| detection.radar_track_id)
                .collect();
            detections.retain(|detection| !held_ids.contains(&detection.radar_track_id));
            detections.extend(self.held_cutins.iter().cloned());
            self.debug_text = "LOG CUT-IN: YES | recorded event hold".into();
        } else {
            self.held_cutins.clear();
            self.debug_text = "LOG CUT-IN: NO".into();
        }
        self.detections = detections;
        self.detection_time = time;
    }
}
fn lead_detection(lead: Data<'_>, label: &str, cut: bool, speed: f64) -> Option<DetectedVehicle> {
    if !lead.flag("status", false) {
        return None;
    }
    let longitudinal = lead.number("dRel", 0.0);
    let lateral = -lead.number("yRel", 0.0);
    if !(0.0..=180.0).contains(&longitudinal) || radar_position_is_zero(longitudinal, lateral) {
        return None;
    }
    let relative = lead.field("vRel").float();
    let absolute = lead
        .field("vLead")
        .float()
        .map(|value| value * 3.6)
        .or_else(|| relative.map(|value| speed + value * 3.6));
    Some(DetectedVehicle {
        label: label.into(),
        longitudinal_m: longitudinal,
        lateral_m: lateral.clamp(-8.0, 8.0),
        source: "radarState".into(),
        relative_speed_mps: relative,
        absolute_speed_kph: absolute,
        lateral_speed_mps: lead.field("vLat").float().map(|value| -value),
        acceleration_mps2: lead.field("aLeadK").float(),
        cut_in: cut,
        primary: true,
        ttc_s: ttc(longitudinal, relative),
        radar_track_id: lead.field("radarTrackId").int(),
        ..DetectedVehicle::default()
    })
}
