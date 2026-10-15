use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Deserialize)]
struct Track {
    #[serde(rename = "box")]
    bounds: [f64; 4],
    raw: String,
    id: Option<i64>,
    #[serde(default)]
    seen_red: bool,
}
#[derive(Clone, Debug)]
struct Anchor {
    bounds: [f64; 4],
    id: Option<i64>,
    red: bool,
    pending: String,
    count: u64,
    since: f64,
}
#[derive(Debug, Default, Serialize)]
pub struct Decision {
    pub hold: bool,
    pub warn: bool,
    pub released: bool,
    pub reason: String,
    pub green_confirmed: bool,
}
#[derive(Debug)]
pub struct Guard {
    hold: bool,
    warned: bool,
    session: Option<String>,
    bounds: Option<[f64; 4]>,
    frame: i64,
    timestamp: f64,
    pending: String,
    since: f64,
    count: u64,
    anchors: Vec<Anchor>,
    track_id: Option<i64>,
    release_authorized: bool,
    green_evidence: bool,
    last_now: f64,
    cooldown: f64,
}
impl Default for Guard {
    fn default() -> Self {
        Self {
            hold: false,
            warned: false,
            session: None,
            bounds: None,
            frame: -1,
            timestamp: f64::NEG_INFINITY,
            pending: String::new(),
            since: 0.,
            count: 0,
            anchors: Vec::new(),
            track_id: None,
            release_authorized: false,
            green_evidence: false,
            last_now: f64::NEG_INFINITY,
            cooldown: f64::NEG_INFINITY,
        }
    }
}
fn front(b: [f64; 4]) -> bool {
    b.iter().all(|v| v.is_finite())
        && 0. <= b[0]
        && b[0] < b[2]
        && b[2] <= 1344.
        && 0. <= b[1]
        && b[1] < b[3]
        && b[3] <= 760.
        && (0.3..=0.75).contains(&((b[0] + b[2]) / 2688.))
        && (0.08..=0.55).contains(&((b[1] + b[3]) / 1520.))
}
fn intersection(a: [f64; 4], b: [f64; 4]) -> f64 {
    (a[2].min(b[2]) - a[0].max(b[0])).max(0.) * (a[3].min(b[3]) - a[1].max(b[1])).max(0.)
}
fn matches(a: [f64; 4], b: [f64; 4]) -> bool {
    let i = intersection(a, b);
    i / ((a[2] - a[0]) * (a[3] - a[1]) + (b[2] - b[0]) * (b[3] - b[1]) - i) >= 0.6
}
fn anchor_matches(b: [f64; 4], id: Option<i64>, t: &Track) -> bool {
    if matches(b, t.bounds) {
        return true;
    }
    if id.is_none() || id != t.id || !t.seen_red {
        return false;
    }
    let o = t.bounds;
    let (w, h, ow, oh) = (b[2] - b[0], b[3] - b[1], o[2] - o[0], o[3] - o[1]);
    let i = intersection(b, o);
    (0.5..=2.).contains(&(ow * oh / (w * h)))
        && i / (w * h + ow * oh - i) >= 0.4
        && ((b[0] + b[2] - o[0] - o[2]) / 2.).abs() <= 0.25 * w
        && ((b[1] + b[3] - o[1] - o[3]) / 2.).abs() <= 0.25 * h
}
fn separate(red: [f64; 4], a: [f64; 4], b: [f64; 4]) -> bool {
    red[2] - red[0] <= 0.7 * (a[2] - a[0]).min(b[2] - b[0])
        && red[3] - red[1] <= 0.7 * (a[3] - a[1]).min(b[3] - b[1])
        && red[1] >= a[3].max(b[3]) + 2. * (a[3] - a[1]).max(b[3] - b[1])
}
impl Guard {
    fn reset(&mut self) {
        let (last_now, cooldown) = (self.last_now, self.cooldown);
        *self = Self::default();
        self.last_now = last_now;
        self.cooldown = cooldown;
    }
    fn reset_votes(&mut self) {
        self.green_evidence = false;
        self.pending.clear();
        self.count = 0;
        for a in &mut self.anchors {
            a.pending.clear();
            a.count = 0;
        }
    }
    fn group(&mut self, tracks: &[Track], ts: f64, gap: f64) -> Option<&'static str> {
        let reds: Vec<_> = tracks.iter().filter(|t| t.raw == "red").collect();
        for t in &reds {
            if self.anchors.len() < 8
                && !self
                    .anchors
                    .iter()
                    .any(|a| anchor_matches(a.bounds, a.id, t))
            {
                self.anchors.push(Anchor {
                    bounds: t.bounds,
                    id: t.id,
                    red: false,
                    pending: String::new(),
                    count: 0,
                    since: ts,
                });
            }
        }
        let mut greens = Vec::new();
        for a in &mut self.anchors {
            let candidates: Vec<_> = tracks
                .iter()
                .filter(|t| anchor_matches(a.bounds, a.id, t))
                .collect();
            let mut color = if candidates.len() == 1 {
                candidates[0].raw.as_str()
            } else {
                "unknown"
            };
            if color == "green"
                && (!a.red || reds.iter().any(|t| !separate(t.bounds, a.bounds, a.bounds)))
            {
                color = "unknown";
            }
            if !["red", "green"].contains(&color) || color != a.pending || gap > 0.7 {
                a.pending = color.into();
                a.since = ts;
                a.count = 1;
            } else {
                a.count += 1;
            }
            if color == "red" {
                a.bounds = candidates[0].bounds;
                a.id = candidates[0].id;
                if a.count >= 2 && ts - a.since >= 0.15 - 1e-9 {
                    a.red = true;
                }
            } else if color == "green" && a.count >= 3 && ts - a.since >= 0.7 - 1e-9 {
                greens.push(a.bounds);
            }
        }
        if reds.is_empty()
            && greens
                .iter()
                .any(|b| (0.45..=0.55).contains(&((b[0] + b[2]) / 2688.)))
        {
            return Some("green_transition");
        }
        for (i, a) in greens.iter().enumerate() {
            for b in &greens[i + 1..] {
                let (aw, bw) = (a[2] - a[0], b[2] - b[0]);
                if a[2].min(b[2]) <= a[0].max(b[0])
                    && aw.max(bw) <= 2. * aw.min(bw)
                    && ((a[1] + a[3] - b[1] - b[3]) / 2.).abs() <= (a[3] - a[1]).max(b[3] - b[1])
                    && reds.iter().all(|t| separate(t.bounds, *a, *b))
                {
                    return Some("green_group");
                }
            }
        }
        None
    }
    fn observe(&mut self, now: f64, value: Option<&Value>) -> &'static str {
        let Some(value) = value.filter(|v| match v {
            Value::Null => false,
            Value::Bool(v) => *v,
            Value::Array(v) => !v.is_empty(),
            Value::Object(v) => !v.is_empty(),
            Value::String(v) => !v.is_empty(),
            Value::Number(v) => v.as_f64() != Some(0.),
        }) else {
            return "missing";
        };
        // Parse time before identity/tracks to retain source stale-data precedence.
        let Some(ts) = value.get("timestamp").and_then(super::transport::numeric) else {
            self.reset_votes();
            return "malformed";
        };
        if !ts.is_finite() || !(0. ..=0.65).contains(&(now - ts)) {
            self.reset_votes();
            return "stale_or_future";
        }
        let frame = value
            .get("frame_id")
            .and_then(|v| v.as_i64().or_else(|| v.as_bool().map(i64::from)));
        let session = value.get("session").and_then(Value::as_str);
        let (Some(frame), Some(session)) = (frame, session) else {
            self.reset_votes();
            return "malformed";
        };
        if session.is_empty() {
            self.reset_votes();
            return "malformed";
        }
        if self.session.as_deref() != Some(session) {
            self.session = Some(session.into());
            self.bounds = None;
            self.track_id = None;
            self.release_authorized = false;
            self.green_evidence = false;
            self.timestamp = f64::NEG_INFINITY;
            self.frame = -1;
            self.pending.clear();
            self.count = 0;
            self.anchors.clear();
        }
        if ts <= self.timestamp || frame <= self.frame {
            return "repeated_or_reordered";
        }
        let gap = ts - self.timestamp;
        self.timestamp = ts;
        self.frame = frame;
        let mut tracks = Vec::new();
        let Some(values) = value.get("tracks").and_then(Value::as_array) else {
            self.reset_votes();
            return "malformed";
        };
        for value in values {
            if value.get("box").is_none() {
                self.reset_votes();
                return "malformed";
            }
            let Some(bounds) = value
                .get("box")
                .and_then(|v| serde_json::from_value::<[f64; 4]>(v.clone()).ok())
            else {
                continue;
            };
            if !front(bounds) {
                continue;
            }
            let Some(raw) = value.get("raw") else {
                self.reset_votes();
                return "malformed";
            };
            tracks.push(Track {
                bounds,
                raw: raw.as_str().unwrap_or("unknown").into(),
                id: value.get("id").and_then(Value::as_i64),
                seen_red: value
                    .get("seen_red")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            });
        }
        let group = self.group(&tracks, ts, gap);
        let candidates: Vec<_> = tracks
            .iter()
            .filter(|t| {
                self.bounds
                    .is_some_and(|b| anchor_matches(b, self.track_id, t))
            })
            .collect();
        let reds: Vec<_> = tracks.iter().filter(|t| t.raw == "red").collect();
        let selected = if self.bounds.is_none() || (candidates.is_empty() && !reds.is_empty()) {
            let s = reds.iter().copied().min_by(|a, b| {
                (((a.bounds[0] + a.bounds[2]) / 2688.) - 0.5)
                    .abs()
                    .total_cmp(&(((b.bounds[0] + b.bounds[2]) / 2688.) - 0.5).abs())
            });
            if let Some(t) = s {
                self.bounds = Some(t.bounds);
                self.track_id = t.id;
                self.pending.clear();
                self.count = 0;
            }
            s
        } else {
            candidates
                .iter()
                .copied()
                .find(|t| t.raw == "red")
                .or_else(|| {
                    if candidates.len() == 1 {
                        Some(candidates[0])
                    } else {
                        None
                    }
                })
        };
        let mut color = selected.map_or("unknown", |t| t.raw.as_str());
        if color == "red" {
            let t = selected.unwrap();
            self.bounds = Some(t.bounds);
            self.track_id = t.id;
        }
        if color == "green" && !reds.is_empty() {
            color = "unknown";
        }
        if let Some(g) = group {
            return g;
        }
        if !["red", "green"].contains(&color) || color != self.pending || gap > 0.7 {
            self.pending = color.into();
            self.since = ts;
            self.count = 1;
        } else {
            self.count += 1;
        }
        if color == "red" && self.count >= 2 && ts - self.since >= 0.15 - 1e-9 {
            return "red";
        }
        if color == "green" && self.count >= 3 && ts - self.since >= 0.7 - 1e-9 {
            return "green";
        }
        "unconfirmed"
    }
    /// Complete policy state for deterministic replay diagnostics.
    pub fn snapshot(&self) -> Value {
        fn pending(v: &str) -> Option<&str> {
            if v.is_empty() {
                None
            } else {
                Some(v)
            }
        }
        serde_json::json!({"hold":self.hold,"warned":self.warned,"session":self.session,"box":self.bounds,
            "frame":self.frame,"timestamp":self.timestamp,"pending":pending(&self.pending),"since":self.since,"count":self.count,
            "red_anchors":self.anchors.iter().map(|a|serde_json::json!({"box":a.bounds,"id":a.id,"red":a.red,"pending":pending(&a.pending),"count":a.count,"since":a.since})).collect::<Vec<_>>(),
            "track_id":self.track_id,"release_authorized":self.release_authorized,"green_evidence":self.green_evidence,
            "last_now":self.last_now,"cooldown":self.cooldown})
    }
    pub fn update(&mut self, input: &Input) -> Decision {
        let Input {
            now,
            enabled,
            valid,
            drive,
            gas,
            speed,
            departure,
            ..
        } = *input;
        let decision = |hold, reason: &str| Decision {
            hold,
            reason: reason.into(),
            ..Decision::default()
        };
        if !enabled || !drive {
            self.reset();
            self.last_now = if now.is_finite() {
                now
            } else {
                f64::NEG_INFINITY
            };
            return decision(false, "disabled");
        }
        if gas {
            self.reset();
            self.last_now = if now.is_finite() {
                now
            } else {
                f64::NEG_INFINITY
            };
            self.cooldown = if now.is_finite() {
                now + 10.
            } else {
                f64::INFINITY
            };
            return decision(false, "driver_gas");
        }
        if !now.is_finite() || now < self.last_now {
            self.pending.clear();
            self.count = 0;
            self.bounds = None;
            self.session = None;
            self.anchors.clear();
            self.release_authorized = false;
            self.green_evidence = false;
            return decision(self.hold, "clock_discontinuity");
        }
        self.last_now = now;
        if now < self.cooldown {
            return decision(false, "driver_cooldown");
        }
        if !valid || !speed.is_finite() {
            self.reset_votes();
            return decision(self.hold, "invalid_context");
        }
        if !self.hold && speed.abs() > 0.3 && !(self.release_authorized && speed.abs() < 5. / 3.6) {
            self.reset();
            return decision(false, "moving_no_entry");
        }
        let evidence = self.observe(now, input.observation.as_ref());
        let mut released = false;
        let green = ["green", "green_group", "green_transition"].contains(&evidence);
        if evidence != "repeated_or_reordered" {
            self.green_evidence = green;
        }
        if evidence == "red" {
            self.release_authorized = false;
            self.hold = self.hold || speed.abs() <= 0.3;
        } else if green && self.hold {
            released = true;
            self.hold = false;
            self.warned = false;
            self.release_authorized = true;
        }
        let warn = self.hold && departure && !self.warned;
        self.warned |= warn;
        Decision {
            hold: self.hold,
            warn,
            released,
            reason: evidence.into(),
            green_confirmed: self.release_authorized
                && self.green_evidence
                && !self.hold
                && (0. ..=0.65).contains(&(now - self.timestamp)),
        }
    }
}
#[derive(Debug, Deserialize)]
pub struct Input {
    pub now: f64,
    pub observation: Option<Value>,
    pub enabled: bool,
    pub valid: bool,
    pub drive: bool,
    pub gas: bool,
    pub speed: f64,
    pub departure: bool,
}
pub fn observation_color(input: &Input) -> u8 {
    if !(input.enabled && input.valid && input.drive && input.now.is_finite()) {
        return 0;
    }
    let Some(value) = input.observation.as_ref() else {
        return 0;
    };
    let Some(ts) = value.get("timestamp").and_then(super::transport::numeric) else {
        return 0;
    };
    if !ts.is_finite() || !(0. ..=0.65).contains(&(input.now - ts)) {
        return 0;
    }
    let Some(tracks) = value.get("tracks").and_then(Value::as_array) else {
        return 0;
    };
    let mut color = 0;
    for t in tracks {
        let Some(b) = t
            .get("box")
            .and_then(|v| serde_json::from_value::<[f64; 4]>(v.clone()).ok())
        else {
            return 0;
        };
        if front(b) {
            match t.get("raw").and_then(Value::as_str) {
                Some("red") => color |= 1,
                Some("green") => color |= 2,
                Some(_) => {}
                None => return 0,
            }
        }
    }
    color
}
