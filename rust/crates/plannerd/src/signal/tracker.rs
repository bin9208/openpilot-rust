//! Bounded housing identities and color confirmation from current image detections.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
#[derive(Clone, Debug, Serialize)]
struct Track {
    #[serde(rename = "ident")]
    id: u64,
    #[serde(rename = "box")]
    bounds: [f64; 4],
    last_seen: f64,
    born: f64,
    state: String,
    pending: String,
    pending_since: f64,
    observations: u64,
    pending_count: u64,
    seen_red: bool,
    last_support: f64,
    evidence: Value,
}
#[derive(Debug)]
pub struct Tracker {
    tracks: Vec<Track>,
    next_id: u64,
    last_timestamp: Option<f64>,
}
impl Default for Tracker {
    fn default() -> Self {
        Self {
            tracks: Vec::new(),
            next_id: 1,
            last_timestamp: None,
        }
    }
}
#[derive(Debug, Deserialize)]
pub struct Frame {
    pub timestamp: f64,
    pub detections: Vec<Value>,
}
pub fn overlap(a: [f64; 4], b: [f64; 4]) -> f64 {
    let intersection =
        (a[2].min(b[2]) - a[0].max(b[0])).max(0.) * (a[3].min(b[3]) - a[1].max(b[1])).max(0.);
    intersection
        / ((a[2] - a[0]) * (a[3] - a[1]) + (b[2] - b[0]) * (b[3] - b[1]) - intersection).max(1.)
}
impl Tracker {
    /// Motion-context rejection clears confirmed and pending green independently.
    pub fn reject_green(&mut self) {
        for track in &mut self.tracks {
            if track.state == "green" {
                track.state = "unknown".into();
            }
            if track.pending == "green" {
                track.pending = "unknown".into();
                track.pending_count = 0;
            }
        }
    }
    /// Complete state of the production update core (pixel history is external).
    pub fn snapshot(&self) -> Value {
        json!({"daytime_cores":true,"robust_tracking":true,"conservative_green":false,"verified_lamps":true,
            "identity_gap":0.5,"tracks":self.tracks,"next_id":self.next_id,"last_timestamp":self.last_timestamp,"previous_gray":null})
    }
    pub fn update(&mut self, frame: &Frame) -> Result<Value, serde_json::Error> {
        let timestamp = frame.timestamp;
        // Production selects robust tracking and verified lamps; legacy observer
        // configurations are separate experiment inputs, not runtime settings.
        if self
            .last_timestamp
            .is_some_and(|last| timestamp <= last || timestamp - last > 0.5)
        {
            self.tracks.clear();
        }
        if self
            .last_timestamp
            .is_some_and(|last| timestamp - last > 0.25)
        {
            for t in &mut self.tracks {
                if t.pending == "red" {
                    continue;
                }
                t.state = "unknown".into();
                t.pending = "unknown".into();
                t.pending_count = 0;
            }
        }
        self.last_timestamp = Some(timestamp);
        self.tracks.retain(|t| {
            timestamp - t.last_seen
                <= if t.seen_red || t.pending == "red" {
                    0.5
                } else {
                    0.25
                }
        });
        let mut used = Vec::new();
        for d in &frame.detections {
            let b: [f64; 4] = serde_json::from_value(d["box"].clone())?;
            let raw = d["raw"].as_str().unwrap_or("unknown");
            let continuity = d.get("continuity_id").and_then(Value::as_u64);
            let mut candidates = Vec::new();
            for (index, t) in self.tracks.iter().enumerate() {
                if used.contains(&t.id) {
                    continue;
                }
                if d.get("continuity_id").is_some() && continuity != Some(t.id) {
                    continue;
                }
                let a = t.bounds;
                let (aw, ah, bw, bh) = (a[2] - a[0], a[3] - a[1], b[2] - b[0], b[3] - b[1]);
                let distance =
                    ((a[0] + a[2] - b[0] - b[2]) / 2.).hypot((a[1] + a[3] - b[1] - b[3]) / 2.);
                let reacquire = matches!(
                    d["source"].as_str(),
                    Some("detected" | "day_color_core" | "night_lamp_core")
                ) && (0.8..=1.25).contains(&(bw / aw))
                    && (0.75..=1.33).contains(&(bh / ah))
                    && distance <= 2_f64.max(aw * 0.15)
                    && overlap(a, b) >= 0.6;
                if timestamp - t.last_seen > 0.25 && continuity != Some(t.id) && !reacquire {
                    continue;
                }
                let limit = if continuity == Some(t.id) {
                    10_f64.max((aw / 5.).floor()) + 1.
                } else {
                    6_f64.max(aw * 0.3)
                };
                let red_motion = continuity == Some(t.id)
                    && d["red_motion_verified"].as_bool().unwrap_or(false)
                    && raw == "red"
                    && t.pending == "red"
                    && d["match_score"].as_f64().unwrap_or(0.) >= 0.9
                    && timestamp - t.last_seen <= 0.25;
                if (0.65..=1.55).contains(&(bw / aw))
                    && (0.5..=2.).contains(&(bh / ah))
                    && distance < limit
                    && (overlap(a, b) > 0.12 || red_motion)
                {
                    candidates.push((distance, index));
                }
            }
            if candidates.len() > 1
                && candidates
                    .iter()
                    .any(|(_, i)| timestamp - self.tracks[*i].last_seen > 0.25)
            {
                candidates.clear();
            }
            let index = if let Some((_, i)) = candidates.iter().min_by(|a, b| a.0.total_cmp(&b.0)) {
                *i
            } else {
                let id = self.next_id;
                self.next_id += 1;
                self.tracks.push(Track {
                    id,
                    bounds: b,
                    last_seen: timestamp,
                    born: timestamp,
                    state: "unknown".into(),
                    pending: "unknown".into(),
                    pending_since: 0.,
                    observations: 0,
                    pending_count: 0,
                    seen_red: false,
                    last_support: 0.,
                    evidence: json!({}),
                });
                self.tracks.len() - 1
            };
            let t = &mut self.tracks[index];
            used.push(t.id);
            let gap = timestamp - t.last_seen;
            t.bounds = b;
            t.last_seen = timestamp;
            t.observations += 1;
            t.evidence = d.clone();
            let confirmation_gap = if raw == "red" { 0.5 } else { 0.25 };
            if raw == "unknown" {
                t.pending = "unknown".into();
                t.pending_count = 0;
                if timestamp - t.last_support > 0.15 {
                    t.state = "unknown".into();
                }
            } else if raw != t.pending || gap > confirmation_gap {
                t.pending = raw.into();
                t.pending_since = timestamp;
                t.pending_count = 1;
                if raw != t.state || gap > confirmation_gap {
                    t.state = "unknown".into();
                }
            } else {
                t.pending_count += 1;
                if timestamp - t.pending_since >= (if raw == "red" { 0.10 } else { 0.15 }) - 1e-6
                    && t.pending_count >= 3
                {
                    if raw == "red" {
                        t.seen_red = true;
                    }
                    t.state = if raw == "red" || t.seen_red {
                        raw
                    } else {
                        "unknown"
                    }
                    .into();
                }
            }
            if raw == t.state && raw != "unknown" {
                t.last_support = timestamp;
            }
        }
        let mut states = Vec::new();
        let visible:Vec<_>=self.tracks.iter().map(|t|{
            let age=timestamp-t.last_seen;
            let state=if age<=0.125{t.state.as_str()}else{"unknown"};
            if state!="unknown"&&t.observations>=3&&!states.contains(&state) {states.push(state);}
            json!({"id":t.id,"box":t.bounds,"state":state,"age":age,"observations":t.observations,"seen_red":t.seen_red,"support_state":t.pending,"support_since":t.pending_since,"support_count":t.pending_count,"evidence":t.evidence})
        }).collect();
        Ok(
            json!({"state":if states.len()==1{states[0]}else{"unknown"},"reason":if states.len()==1{"agreeing_visible_tracks"}else{"conflict_or_unconfirmed"},"tracks":visible}),
        )
    }
}
