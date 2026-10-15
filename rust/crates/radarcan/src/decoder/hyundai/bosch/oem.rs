//! Bounded OEM longitudinal intent and publication-only candidate probation.
use super::physical::Object;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Intent {
    #[serde(rename = "_samples")]
    pub samples: VecDeque<(i64, f64)>,
    pub last_ns: i64,
}
impl Intent {
    pub fn reset(&mut self) {
        self.samples.clear();
        self.last_ns = 0;
    }
    pub fn ingest(&mut self, timestamp: i64, value: Option<f64>) {
        let Some(value) = value.filter(|v| v.is_finite() && (-9.0..=3.0).contains(v)) else {
            return;
        };
        if self.samples.back().is_some_and(|s| timestamp < s.0) {
            self.samples.clear();
        }
        self.samples.push_back((timestamp, value));
        self.last_ns = timestamp;
        let horizon = timestamp - 300_000_000;
        while self.samples.front().is_some_and(|s| s.0 < horizon) {
            self.samples.pop_front();
        }
    }
    pub fn median(&self, timestamp: i64) -> Option<f64> {
        if self.samples.is_empty() || timestamp - self.last_ns > 150_000_000 {
            return None;
        }
        let horizon = timestamp - 300_000_000;
        let mut values: Vec<_> = self
            .samples
            .iter()
            .filter(|s| s.0 >= horizon)
            .map(|s| s.1)
            .collect();
        if values.len() < 3 {
            return None;
        }
        values.sort_by(|a, b| a.partial_cmp(b).unwrap());
        Some(values[values.len() / 2])
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct State {
    pub tentative_scans: usize,
    pub since_ns: i64,
    pub last_ns: i64,
    pub evidence_ns: i64,
    pub ever_validated: bool,
    pub in_path_scans: usize,
    pub last_abs_y: f64,
    pub inward_scans: usize,
    pub inward_from_abs_y: f64,
    pub inward_since_ns: i64,
    pub withheld: bool,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Gate {
    pub mode: u8,
    pub neutral_mps2: f64,
    pub persist_scans: usize,
    pub min_speed_mps: f64,
    pub min_range_m: f64,
    pub min_ttc_s: f64,
    pub cutin_mps: f64,
    pub cutin_scans: usize,
    pub evidence_hold_ns: i64,
    pub in_path_m: f64,
    pub settled_scans: usize,
    pub sibling_d_m: f64,
    pub sibling_y_m: f64,
    pub sibling_v_mps: f64,
    pub intent: Intent,
    #[serde(rename = "_states")]
    pub states: IndexMap<u32, State>,
    pub last_ns: Option<i64>,
    pub state: u8,
    pub would_withhold: BTreeSet<u32>,
    pub reasons: BTreeMap<u32, Value>,
    pub publication_withheld: usize,
    pub state_peak: usize,
}
impl Gate {
    pub fn new(mode: u8) -> Result<Self, &'static str> {
        if mode > 2 {
            return Err("invalid Bosch OEM validation gate mode");
        }
        Ok(Self {
            mode,
            neutral_mps2: -0.25,
            persist_scans: 3,
            min_speed_mps: 8.,
            min_range_m: 30.,
            min_ttc_s: 4.,
            cutin_mps: 0.90,
            cutin_scans: 3,
            evidence_hold_ns: 500_000_000,
            in_path_m: 1.,
            settled_scans: 10,
            sibling_d_m: 12.,
            sibling_y_m: 3.,
            sibling_v_mps: 1.5,
            intent: Intent::default(),
            states: IndexMap::new(),
            last_ns: None,
            state: 0,
            would_withhold: BTreeSet::new(),
            reasons: BTreeMap::new(),
            publication_withheld: 0,
            state_peak: 0,
        })
    }
    pub fn classify(word0: bool, word1: bool) -> u8 {
        match (word0, word1) {
            (false, false) => 0,
            (true, false) => 1,
            (false, true) => 2,
            (true, true) => 3,
        }
    }
    pub fn update(
        &mut self,
        objects: &[Object],
        timestamp: i64,
        ego: f64,
        state: u8,
        word0: &BTreeSet<u32>,
        valid: Option<bool>,
    ) -> BTreeSet<u32> {
        self.last_ns = Some(timestamp);
        self.state = state;
        self.would_withhold.clear();
        self.reasons.clear();
        if self.mode == 0 {
            self.states.clear();
            return self.would_withhold.clone();
        }
        let live: BTreeSet<_> = objects.iter().map(|o| o.physical_track_id).collect();
        let mut states = IndexMap::new();
        let validated = state == 3 && valid != Some(false);
        for obj in objects {
            let pid = obj.physical_track_id;
            let prior = self.states.get(&pid);
            let mut evidence = prior.map_or(0, |s| s.evidence_ns);
            let mut ever = prior.is_some_and(|s| s.ever_validated);
            if obj.vision_supported
                || obj.oem_selected
                || obj.members.len() > 1
                || (validated && word0.contains(&pid))
            {
                evidence = timestamp;
            }
            if obj.oem_selected || (validated && word0.contains(&pid)) {
                ever = true;
            }
            let abs_y = obj.y_rel.abs();
            let in_path = if abs_y <= self.in_path_m {
                prior.map_or(1, |s| s.in_path_scans + 1)
            } else {
                0
            };
            let (inward_scans, inward_from, inward_since) = match prior {
                Some(p) if timestamp - p.last_ns <= 600_000_000 && abs_y < p.last_abs_y => (
                    p.inward_scans + 1,
                    if p.inward_scans > 0 {
                        p.inward_from_abs_y
                    } else {
                        p.last_abs_y
                    },
                    if p.inward_scans > 0 {
                        p.inward_since_ns
                    } else {
                        p.last_ns
                    },
                ),
                _ => (0, abs_y, timestamp),
            };
            states.insert(
                pid,
                State {
                    tentative_scans: prior.map_or(0, |s| s.tentative_scans),
                    since_ns: prior.map_or(timestamp, |s| s.since_ns),
                    last_ns: timestamp,
                    evidence_ns: evidence,
                    ever_validated: ever,
                    in_path_scans: in_path,
                    last_abs_y: abs_y,
                    inward_scans,
                    inward_from_abs_y: inward_from,
                    inward_since_ns: inward_since,
                    withheld: prior.is_some_and(|s| s.withheld),
                },
            );
        }
        let eligible = if state == 1 && word0.len() == 1 {
            word0.intersection(&live).copied().collect::<BTreeSet<_>>()
        } else {
            BTreeSet::new()
        };
        let intent = self.intent.median(timestamp);
        let confirmed: Vec<_> = objects
            .iter()
            .filter(|o| states[&o.physical_track_id].ever_validated)
            .collect();
        let mut withhold = BTreeSet::new();
        for obj in objects {
            let pid = obj.physical_track_id;
            let entry = states.get_mut(&pid).unwrap();
            if !eligible.contains(&pid) {
                entry.tentative_scans = 0;
                entry.since_ns = timestamp;
                entry.withheld = false;
                continue;
            }
            entry.tentative_scans += 1;
            if entry.tentative_scans == 1 {
                entry.since_ns = timestamp;
            }
            let rate = if entry.inward_scans >= self.cutin_scans {
                (entry.inward_from_abs_y - entry.last_abs_y)
                    / ((timestamp - entry.inward_since_ns) as f64 * 1e-9).max(1e-3)
            } else {
                0.
            };
            let closing = -obj.v_rel;
            let ttc = if closing > 0.1 {
                obj.d_rel / closing
            } else {
                f64::INFINITY
            };
            let fail_open = if entry.ever_validated {
                Some("was_validated")
            } else if entry.in_path_scans > self.settled_scans {
                Some("settled_in_path")
            } else if obj.y_rel.abs() > self.in_path_m {
                Some("not_in_path")
            } else if confirmed.iter().any(|other| {
                other.physical_track_id != pid
                    && (obj.d_rel - other.d_rel).abs() <= self.sibling_d_m
                    && (obj.y_rel - other.y_rel).abs() <= self.sibling_y_m
                    && (obj.v_rel - other.v_rel).abs() <= self.sibling_v_mps
            }) {
                Some("validated_sibling")
            } else if valid == Some(true) {
                Some("scc_valid")
            } else if !ego.is_finite() || ego < self.min_speed_mps {
                Some("low_speed")
            } else if obj.d_rel < self.min_range_m {
                Some("close_range")
            } else if ttc < self.min_ttc_s {
                Some("short_ttc")
            } else if timestamp - entry.evidence_ns <= self.evidence_hold_ns
                && entry.evidence_ns != 0
            {
                Some("recent_evidence")
            } else if rate >= self.cutin_mps {
                Some("lateral_cutin")
            } else if intent.is_none() {
                Some("oem_intent_unavailable")
            } else if intent.is_some_and(|v| v <= self.neutral_mps2) {
                Some("oem_decelerating")
            } else if entry.tentative_scans < self.persist_scans {
                Some("warming")
            } else {
                None
            };
            if fail_open.is_none() {
                withhold.insert(pid);
            }
            entry.withheld = fail_open.is_none();
            self.reasons.insert(pid,json!({"withhold":entry.withheld,"reason":fail_open.unwrap_or("tentative_oem_disagreement"),"tentative_scans":entry.tentative_scans,"in_path_scans":entry.in_path_scans,"d_rel":obj.d_rel,"y_rel":obj.y_rel,"v_rel":obj.v_rel,"ttc_s":float_value(ttc),"lateral_rate_mps":rate,"oem_intent_mps2":intent,"scc_obj_valid":valid,"evidence_age_s":(entry.evidence_ns!=0).then_some((timestamp-entry.evidence_ns) as f64*1e-9)}));
        }
        self.states = states;
        self.state_peak = self.state_peak.max(self.states.len());
        self.would_withhold = withhold;
        self.would_withhold.clone()
    }
}
pub(crate) fn float_value(value: f64) -> Value {
    if value.is_finite() {
        json!(value)
    } else if value.is_nan() {
        json!("NaN")
    } else if value.is_sign_positive() {
        json!("Infinity")
    } else {
        json!("-Infinity")
    }
}
