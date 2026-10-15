//! Causal physical-PID-owned lead acceleration; held scans never update filters.
use super::physical::Object;
use crate::{lead_filter::LeadFilter, Error};
use serde::{ser::SerializeMap, Serialize, Serializer};
use std::collections::{BTreeMap, BTreeSet};
fn floats<S: Serializer>(values: &BTreeMap<u32, f64>, serializer: S) -> Result<S::Ok, S::Error> {
    struct Number(f64);
    impl Serialize for Number {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            crate::scalar::serialize_float(&self.0, s)
        }
    }
    let mut map = serializer.serialize_map(Some(values.len()))?;
    for (&key, &value) in values {
        map.serialize_entry(&key, &Number(value))?;
    }
    map.end()
}
#[derive(Clone, Serialize)]
pub struct State {
    #[serde(serialize_with = "crate::scalar::serialize_float")]
    pub v_lead_filtered: f64,
    pub lead_filter: LeadFilter,
    pub last_scan_ns: i64,
    pub update_count: usize,
    #[serde(serialize_with = "crate::scalar::serialize_float")]
    pub a_lead: f64,
}
#[derive(Serialize)]
pub struct Estimator {
    #[serde(skip)]
    empty: BTreeMap<u32, f64>,
    pub states: BTreeMap<u32, State>,
    #[serde(rename = "_a_lead_by_pid", serialize_with = "floats")]
    pub a_lead_by_pid: BTreeMap<u32, f64>,
    pub last_scan_ns: Option<i64>,
    pub reset_count: usize,
    pub last_reset_reason: String,
    pub last_publication_kind: String,
    pub expired_count: usize,
    pub last_expiry_reason: String,
    pub update_count: usize,
    pub peak_state_count: usize,
    pub allocation_count: usize,
    pub held_publication_count: usize,
    pub held_update_count: usize,
    pub invalid_input_count: usize,
    pub corrupt_state_count: usize,
    pub clock_reset_count: usize,
    pub gap_reset_count: usize,
}
impl Estimator {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn debug_snapshot(&self, now_ns: i64) -> serde_json::Value {
        serde_json::json!({
            "last_scan_ns":self.last_scan_ns,"reset_count":self.reset_count,
            "last_reset_reason":self.last_reset_reason,"last_publication_kind":self.last_publication_kind,
            "expired_count":self.expired_count,"last_expiry_reason":self.last_expiry_reason,
            "update_count":self.update_count,"state_count":self.states.len(),"peak_state_count":self.peak_state_count,
            "allocation_count":self.allocation_count,"held_publication_count":self.held_publication_count,
            "held_update_count":self.held_update_count,"invalid_input_count":self.invalid_input_count,
            "corrupt_state_count":self.corrupt_state_count,"clock_reset_count":self.clock_reset_count,
            "gap_reset_count":self.gap_reset_count,
            "states":self.states.iter().map(|(&pid,state)|serde_json::json!({
                "physical_pid":pid,"source_scan_timestamp_ns":state.last_scan_ns,
                "estimator_update_count":state.update_count,"estimated_aLead":state.a_lead,
                "state_age_ns":(i128::from(now_ns)-i128::from(state.last_scan_ns)).max(0) as u64,
            })).collect::<Vec<_>>(),
        })
    }
    pub fn reset(&mut self, reason: &str) {
        self.states.clear();
        self.a_lead_by_pid.clear();
        self.last_scan_ns = None;
        self.reset_count += 1;
        self.last_reset_reason = reason.into();
        self.last_publication_kind = "RESET".into();
    }
    pub fn update(
        &mut self,
        objects: &[Object],
        scan_ns: Option<i64>,
        v_ego: f64,
        live_pids: &BTreeSet<u32>,
    ) -> Result<&BTreeMap<u32, f64>, Error> {
        let Some(scan_ns) = scan_ns else {
            self.last_publication_kind = "NO_SCAN".into();
            return Ok(&self.empty);
        };
        if scan_ns < 0 {
            self.reset("INVALID_SCAN_TIMESTAMP");
            self.invalid_input_count += objects.len();
            return Ok(&self.a_lead_by_pid);
        }
        if Some(scan_ns) == self.last_scan_ns {
            self.last_publication_kind = "HELD".into();
            self.held_publication_count += 1;
            return Ok(&self.a_lead_by_pid);
        }
        if let Some(last) = self.last_scan_ns {
            if scan_ns < last {
                self.clock_reset_count += 1;
                self.reset("CLOCK_RESET");
            } else if scan_ns - last >= 300_000_000 {
                self.gap_reset_count += 1;
                self.reset("SCAN_GAP");
            }
        }
        self.a_lead_by_pid.clear();
        for object in objects {
            let pid = object.physical_track_id;
            let needs_update = self
                .states
                .get(&pid)
                .is_none_or(|s| s.last_scan_ns != scan_ns);
            if needs_update {
                let v_lead = v_ego
                    + object
                        .published_surface
                        .as_ref()
                        .map_or(object.v_rel, |s| s.v_rel);
                if !v_lead.is_finite() {
                    self.states.remove(&pid);
                    self.a_lead_by_pid.insert(pid, f64::NAN);
                    self.invalid_input_count += 1;
                    continue;
                }
                if let Some(state) = self.states.get_mut(&pid) {
                    state.v_lead_filtered = 0.5 * state.v_lead_filtered + 0.5 * v_lead;
                    let stationary = state.v_lead_filtered.abs() < 0.3
                        && (v_lead - state.v_lead_filtered).abs() < 0.05;
                    let filtered = state.lead_filter.update(v_lead, stationary)?;
                    if !filtered.is_finite() {
                        self.states.remove(&pid);
                        self.a_lead_by_pid.insert(pid, f64::NAN);
                        self.corrupt_state_count += 1;
                        continue;
                    }
                    state.last_scan_ns = scan_ns;
                    state.update_count += 1;
                    state.a_lead = if state.update_count >= 6 {
                        filtered
                    } else {
                        0.
                    };
                } else {
                    self.states.insert(
                        pid,
                        State {
                            v_lead_filtered: v_lead,
                            lead_filter: LeadFilter::new(v_lead, 0.1)?,
                            last_scan_ns: scan_ns,
                            update_count: 1,
                            a_lead: 0.,
                        },
                    );
                    self.allocation_count += 1;
                }
                self.update_count += 1;
            }
            self.a_lead_by_pid.insert(pid, self.states[&pid].a_lead);
        }
        let expired: Vec<_> = self
            .states
            .keys()
            .copied()
            .filter(|pid| !live_pids.contains(pid) && !self.a_lead_by_pid.contains_key(pid))
            .collect();
        for pid in expired {
            self.states.remove(&pid);
            self.expired_count += 1;
            self.last_expiry_reason = "PID_DEATH".into();
        }
        if self.states.len() > 128 {
            let mut victims: Vec<_> = self
                .states
                .iter()
                .map(|(&pid, s)| (self.a_lead_by_pid.contains_key(&pid), s.last_scan_ns, pid))
                .collect();
            victims.sort_unstable();
            for &(_, _, pid) in &victims[..self.states.len() - 128] {
                self.states.remove(&pid);
                if let Some(value) = self.a_lead_by_pid.get_mut(&pid) {
                    *value = f64::NAN;
                }
                self.expired_count += 1;
                self.last_expiry_reason = "STATE_CAP".into();
            }
        }
        self.last_scan_ns = Some(scan_ns);
        self.last_publication_kind = "FRESH".into();
        self.peak_state_count = self.peak_state_count.max(self.states.len());
        Ok(&self.a_lead_by_pid)
    }
}

impl Default for Estimator {
    fn default() -> Self {
        Self {
            empty: BTreeMap::new(),
            states: BTreeMap::new(),
            a_lead_by_pid: BTreeMap::new(),
            last_scan_ns: None,
            reset_count: 0,
            last_reset_reason: "INITIAL".into(),
            last_publication_kind: "INITIAL".into(),
            expired_count: 0,
            last_expiry_reason: "NONE".into(),
            update_count: 0,
            peak_state_count: 0,
            allocation_count: 0,
            held_publication_count: 0,
            held_update_count: 0,
            invalid_input_count: 0,
            corrupt_state_count: 0,
            clock_reset_count: 0,
            gap_reset_count: 0,
        }
    }
}
