//! Fresh, identity-specific red preparation; never estimates a stop line.
use super::priority::{self, Candidate, Priority};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
pub trait Geometry {
    fn range(&mut self, bounds: [f64; 4], night: bool) -> Option<[f64; 2]>;
    fn alignment(&mut self, candidate: &Candidate) -> Option<[f64; 2]>;
    fn night(&self) -> bool;
}
#[derive(Debug, Deserialize)]
pub struct Input {
    pub now: f64,
    pub observation: Option<Value>,
    pub enabled: bool,
    pub valid: bool,
    pub drive: bool,
    pub speed: f64,
    pub gas: bool,
    pub brake: bool,
    pub lead: bool,
    pub turning: bool,
    pub model_stopping: bool,
    pub entry_allowed: bool,
}
#[derive(Debug, Serialize)]
pub struct Decision {
    pub active: bool,
    pub entered: bool,
    pub reason: String,
    pub track_id: i64,
    pub distance_range: Option<[f64; 2]>,
    pub track_ids: Vec<i64>,
    pub priority: &'static str,
    pub priority_ids: Vec<i64>,
    pub side_green_ids: Vec<i64>,
    pub evidence_kind: &'static str,
    pub support_interval: Option<[f64; 2]>,
}
impl Decision {
    fn reason(reason: &str) -> Self {
        Self {
            active: false,
            entered: false,
            reason: reason.into(),
            track_id: -1,
            distance_range: None,
            track_ids: Vec::new(),
            priority: "none",
            priority_ids: Vec::new(),
            side_green_ids: Vec::new(),
            evidence_kind: "housing",
            support_interval: None,
        }
    }
}
#[derive(Debug)]
pub struct Prepare {
    session: Option<String>,
    last_frame: i64,
    last_capture: f64,
    histories: BTreeMap<i64, (f64, u64)>,
    priority: Option<Priority>,
    priority_since: Option<f64>,
    priority_count: u64,
    active: bool,
    last_now: Option<f64>,
    cooldown: f64,
    last_sound: f64,
    last_night_support: f64,
    night_anchors: BTreeMap<i64, (f64, f64, [f64; 4])>,
}
impl Default for Prepare {
    fn default() -> Self {
        Self {
            session: None,
            last_frame: -1,
            last_capture: f64::NEG_INFINITY,
            histories: BTreeMap::new(),
            priority: None,
            priority_since: None,
            priority_count: 0,
            active: false,
            last_now: None,
            cooldown: f64::NEG_INFINITY,
            last_sound: f64::NEG_INFINITY,
            last_night_support: f64::NEG_INFINITY,
            night_anchors: BTreeMap::new(),
        }
    }
}
fn number(value: &Value, key: &str) -> Result<f64, &'static str> {
    value.get(key).and_then(Value::as_f64).ok_or("malformed")
}
fn ident(value: &Value) -> Result<i64, &'static str> {
    value.get("id").and_then(Value::as_i64).ok_or("malformed")
}
fn bounds(value: &Value) -> Result<[f64; 4], &'static str> {
    serde_json::from_value(value.get("box").ok_or("malformed")?.clone()).map_err(|_| "malformed")
}
fn valid_box(b: [f64; 4]) -> bool {
    b.iter().all(|v| v.is_finite())
        && 0. <= b[0]
        && b[0] < b[2]
        && b[2] <= 1344.
        && 0. <= b[1]
        && b[1] < b[3]
        && b[3] <= 760.
}
impl Prepare {
    pub fn reset_mode(&mut self) {
        self.reset("mode_change");
    }
    fn reset(&mut self, reason: &str) -> Decision {
        if !["no_red", "too_many_red"].contains(&reason) {
            self.night_anchors.clear();
        }
        self.histories.clear();
        self.priority = None;
        self.priority_since = None;
        self.priority_count = 0;
        self.active = false;
        self.last_night_support = f64::NEG_INFINITY;
        Decision::reason(reason)
    }
    /// Complete policy state for source comparisons and deterministic replay.
    pub fn snapshot(&self) -> Value {
        let priority = self
            .priority
            .as_ref()
            .map(|p| (&p.kind, &p.support, &p.green_ids));
        serde_json::json!({"session":self.session,"last_frame":self.last_frame,"last_capture":self.last_capture,
            "histories":self.histories,"priority_key":priority,"priority_since":self.priority_since,
            "priority_count":self.priority_count,"active":self.active,"last_now":self.last_now,
            "cooldown":self.cooldown,"last_sound":self.last_sound,"last_night_support":self.last_night_support,
            "night_anchors":self.night_anchors})
    }
    pub fn update(&mut self, input: &Input, geometry: &mut impl Geometry) -> Decision {
        let now = input.now;
        if !now.is_finite() || self.last_now.is_some_and(|last| now <= last) {
            return self.reset("clock");
        }
        self.last_now = Some(now);
        if input.gas || input.brake {
            self.cooldown = now + 10.;
        }
        let eligible = input.enabled
            && input.valid
            && input.drive
            && !input.gas
            && !input.brake
            && !input.lead
            && !input.turning
            && !input.model_stopping
            && input.entry_allowed
            && input.speed.is_finite()
            && 5. / 3.6 < input.speed
            && input.speed < 82. / 3.6
            && now >= self.cooldown;
        if !eligible {
            return self.reset("ineligible");
        }
        match self.observe(input, geometry) {
            Ok(value) => value,
            Err(reason) => self.reset(reason),
        }
    }
    fn observe(
        &mut self,
        input: &Input,
        geometry: &mut impl Geometry,
    ) -> Result<Decision, &'static str> {
        let now = input.now;
        let obs = input.observation.as_ref().ok_or("malformed")?;
        let stamp = number(obs, "timestamp")?;
        let frame = obs
            .get("frame_id")
            .and_then(Value::as_i64)
            .ok_or("malformed")?;
        let session = obs.get("session").and_then(Value::as_str).ok_or("stale")?;
        let tracks = obs
            .get("tracks")
            .and_then(Value::as_array)
            .ok_or("malformed")?;
        let mut direct_night = false;
        if geometry.night() {
            for t in tracks {
                if number(t, "age")? == 0.
                    && t["evidence"]["raw"] == "red"
                    && (t["evidence"]["source"] == "night_lamp_core"
                        || self.night_anchors.contains_key(&ident(t)?))
                {
                    direct_night = true;
                }
            }
        }
        let night = geometry.night()
            && (direct_night
                || self.priority.as_ref().is_some_and(|p| {
                    p.kind == "night_red_group" && p.support.is_empty() && p.green_ids.is_empty()
                }));
        let max_age = if night { 0.35 } else { 0.2 };
        if obs["version"] != 1
            || obs["stream"] != "road"
            || obs["size"] != serde_json::json!([1344, 760])
            || session.is_empty()
            || !stamp.is_finite()
            || now < stamp
        {
            return Err("stale");
        }
        if now - stamp > max_age {
            if self.session.as_deref() == Some(session)
                && frame == self.last_frame
                && stamp == self.last_capture
                && now - stamp <= 0.25 + 0.2
            {
                self.active = false;
                return Ok(Decision::reason("awaiting_fresh"));
            }
            return Err("stale");
        }
        if self.session.as_deref() != Some(session) {
            self.reset("new_session");
            self.session = Some(session.into());
            self.last_frame = -1;
            self.last_capture = f64::NEG_INFINITY;
        }
        if frame < self.last_frame || stamp < self.last_capture {
            return Err("reordered");
        }
        let fresh = frame != self.last_frame && stamp > self.last_capture;
        if !fresh && (frame != self.last_frame || stamp != self.last_capture) {
            return Err("inconsistent_frame");
        }
        if fresh {
            if stamp - self.last_capture > 0.25 {
                self.reset("capture_gap");
            }
            self.last_frame = frame;
            self.last_capture = stamp;
        }
        self.night_anchors
            .retain(|_, a| (0. ..=0.5).contains(&(stamp - a.0)));
        if fresh && night {
            for t in tracks {
                let e = &t["evidence"];
                let b = bounds(t)?;
                if !valid_box(b) {
                    return Err("malformed");
                }
                let diameter = e["core_diameter"].as_f64().unwrap_or(0.);
                if number(t, "age")? == 0.
                    && e["raw"] == "red"
                    && e["source"] == "night_lamp_core"
                    && (0.5..=1.).contains(&e["quality"].as_f64().unwrap_or(0.))
                    && (3. ..=22.).contains(&diameter)
                    && (b[2] - b[0] - 4. * diameter).abs() <= 2.
                    && (b[3] - b[1] - 2. * diameter).abs() <= 2.
                {
                    self.night_anchors.insert(ident(t)?, (stamp, diameter, b));
                }
            }
        }
        if night {
            for t in tracks {
                if number(t, "age")? == 0. && t["evidence"]["raw"] == "green" {
                    let b = bounds(t)?;
                    if (0.25 * 1344. ..=0.8 * 1344.).contains(&((b[0] + b[2]) / 2.)) {
                        return Err("night_green_conflict");
                    }
                }
            }
        }
        let mut reds = Vec::new();
        let mut greens = Vec::new();
        for t in tracks {
            let e = &t["evidence"];
            let state = t["state"].as_str().ok_or("malformed")?;
            if number(t, "age")? != 0.
                || !["red", "green"].contains(&state)
                || e["raw"] != state
                || (!night && e["source"] != "detected")
                || number(t, "observations")? < 3.
                || !(0.5..=1.).contains(&e["quality"].as_f64().unwrap_or(0.))
            {
                continue;
            }
            let mut b = bounds(t)?;
            let Some(distance) = geometry.range(b, night) else {
                continue;
            };
            let id = ident(t)?;
            if state == "green" {
                greens.push(Candidate {
                    id,
                    distance,
                    bounds: b,
                });
            } else if (0.4..=1.).contains(&e["red_score"].as_f64().unwrap_or(0.))
                && (0. ..0.08).contains(&e["green_score"].as_f64().unwrap_or(1.))
            {
                if night {
                    let Some(anchor) = self.night_anchors.get(&id) else {
                        continue;
                    };
                    let diameter = if e["source"] == "night_lamp_core" {
                        let diameter = e["core_diameter"].as_f64().unwrap_or(0.);
                        if !(3. ..=22.).contains(&diameter) || anchor.0 != stamp {
                            continue;
                        }
                        diameter
                    } else if e["source"] == "causal_template"
                        && (0.9..=1.).contains(&e["match_score"].as_f64().unwrap_or(0.))
                    {
                        let old = anchor.2;
                        let diameter = anchor.1;
                        if ((b[0] + b[2] - old[0] - old[2]) / 2.).abs() > 2. * diameter
                            || ((b[1] + b[3] - old[1] - old[3]) / 2.).abs() > 2. * diameter
                            || !(0.8..=1.25).contains(&((b[2] - b[0]) / (old[2] - old[0])))
                            || !(0.8..=1.25).contains(&((b[3] - b[1]) / (old[3] - old[1])))
                        {
                            continue;
                        }
                        diameter
                    } else {
                        continue;
                    };
                    let x = b[0] + 0.125 * (b[2] - b[0]);
                    let y = (b[1] + b[3]) / 2.;
                    b = [
                        x - diameter / 2.,
                        y - diameter / 2.,
                        x + diameter / 2.,
                        y + diameter / 2.,
                    ];
                }
                reds.push(Candidate {
                    id,
                    distance,
                    bounds: b,
                });
            }
        }
        let required = if night { 2 } else { 1 };
        if !(required..=4).contains(&reds.len()) {
            if night && reds.len() < 2 && stamp - self.last_night_support <= 0.35 {
                self.active = false;
                let mut d = Decision::reason("night_waiting");
                d.evidence_kind = "night_lamp_group";
                return Ok(d);
            }
            return Err(if reds.len() < required {
                "no_red"
            } else {
                "too_many_red"
            });
        }
        let mut ids: Vec<_> = reds.iter().map(|c| c.id).collect();
        ids.sort_unstable();
        let distance = [
            reds.iter()
                .map(|c| c.distance[0])
                .fold(f64::NEG_INFINITY, f64::max),
            reds.iter()
                .map(|c| c.distance[1])
                .fold(f64::INFINITY, f64::min),
        ];
        if ids.iter().collect::<BTreeSet<_>>().len() != ids.len() || distance[0] >= distance[1] {
            return Err("incompatible_red_group");
        }
        for (i, a) in reds.iter().enumerate() {
            for b in &reds[i + 1..] {
                let (a, b) = (a.bounds, b.bounds);
                let (aw, bw) = (a[2] - a[0], b[2] - b[0]);
                if (a[2].min(b[2]) > a[0].max(b[0]) && a[3].min(b[3]) > a[1].max(b[1]))
                    || aw.max(bw) > 2. * aw.min(bw)
                    || ((a[1] + a[3] - b[1] - b[3]) / 2.).abs()
                        > 2. * (a[3] - a[1]).max(b[3] - b[1])
                {
                    return Err("incompatible_red_group");
                }
            }
        }
        let priority = if greens.is_empty() {
            Priority {
                kind: if night { "night_red_group" } else { "red_only" },
                support: Vec::new(),
                green_ids: Vec::new(),
            }
        } else {
            priority::central(&reds, &greens, |c| geometry.alignment(c)).ok_or("mixed_colors")?
        };
        if self.priority.as_ref() != Some(&priority) {
            if self
                .priority
                .as_ref()
                .is_some_and(|p| (p.kind == "night_red_group") != night)
            {
                self.histories.clear();
            }
            self.priority = Some(priority.clone());
            self.priority_since = None;
            self.priority_count = 0;
        }
        if fresh {
            if night {
                self.last_night_support = stamp;
            }
            self.priority_since.get_or_insert(stamp);
            self.priority_count += 1;
        }
        self.histories.retain(|id, _| ids.contains(id));
        if fresh {
            for id in &ids {
                let entry = self.histories.entry(*id).or_insert((stamp, 0));
                entry.1 += 1;
            }
        }
        let confirmed: Vec<_> = self
            .histories
            .iter()
            .filter(|(_, h)| h.1 >= 4 && stamp - h.0 >= 0.6 - 1e-9)
            .map(|(id, _)| *id)
            .collect();
        let mut id = confirmed.first().copied().unwrap_or(ids[0]);
        let was_active = self.active;
        self.active = if night {
            confirmed.len() >= 2
        } else {
            !confirmed.is_empty()
        };
        if !greens.is_empty() {
            self.active = self.active
                && priority.support.iter().all(|id| confirmed.contains(id))
                && self.priority_count >= 4
                && self.priority_since.is_some_and(|s| stamp - s >= 0.6 - 1e-9);
            id = priority.support[0];
        }
        let entered = self.active && !was_active && now - self.last_sound >= 10.;
        if entered {
            self.last_sound = now;
        }
        Ok(Decision {
            active: self.active,
            entered,
            reason: if self.active {
                "red_prepare"
            } else {
                "confirming"
            }
            .into(),
            track_id: id,
            distance_range: if night { None } else { Some(distance) },
            track_ids: ids,
            priority: priority.kind,
            priority_ids: priority.support,
            side_green_ids: priority.green_ids,
            evidence_kind: if night { "night_lamp_group" } else { "housing" },
            support_interval: if night { Some(distance) } else { None },
        })
    }
}
