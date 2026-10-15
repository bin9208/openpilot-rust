//! Complete passive Bosch receive composition; all publication filters remain separate from tracking.
use super::{
    alias, birth_b5, burst, camera, camera_extended, family, group_config, mirror_birth, mirror_m3,
    oem, persistent_clone,
    physical::{Object, Surface, VisionCue},
    points, raw, road_edge, sidepass, tracker, tracking, unsupported_birth,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub bus: i32,
    pub qualification: bool,
    pub camera_bus: i32,
    pub camera_extended_mode: u8,
    pub p91_mode: u8,
    pub oem_gate_mode: u8,
    pub scc_bus: i32,
    pub curve_reacquire_mode: u8,
    pub provisional_bundle: bool,
    pub family_companion_mode: u8,
    pub burst_multireturn_mode: u8,
    pub b5_mode: i32,
    pub sidepass_lateral_mode: u8,
    pub mirror_birth_mode: u8,
    pub unsupported_birth: bool,
    pub mature_group_retention: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            bus: 0,
            qualification: true,
            camera_bus: 1,
            camera_extended_mode: 3,
            p91_mode: 2,
            oem_gate_mode: 2,
            scc_bus: 2,
            curve_reacquire_mode: 2,
            provisional_bundle: true,
            family_companion_mode: 2,
            burst_multireturn_mode: 2,
            b5_mode: 1,
            sidepass_lateral_mode: 2,
            mirror_birth_mode: 2,
            unsupported_birth: true,
            mature_group_retention: true,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct CanFrame {
    pub timestamp_ns: i64,
    pub address: u32,
    pub payload: Vec<u8>,
    pub order: u64,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Packet {
    pub timestamp_ns: i64,
    pub messages: Vec<Message>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Message {
    pub address: u32,
    pub payload: Vec<u8>,
    pub bus: i32,
}
pub struct Context<'a> {
    pub now: i64,
    pub ego: f64,
    pub yaw: Option<f64>,
    pub vision: &'a [VisionCue],
    pub path: &'a [(f64, f64)],
    pub path_ns: Option<i64>,
    pub path_source_ns: Option<i64>,
}
#[derive(Default)]
pub struct Debug {
    pub objects: Vec<Object>,
    pub phase_ns: Option<i64>,
    pub closed_ns: Option<i64>,
    pub tick: Option<u32>,
    pub complete: bool,
    pub oem_word: Option<u32>,
    pub oem_slot: Option<usize>,
    pub oem_matches: usize,
    pub processed_word: Option<u32>,
    pub processed_pids: BTreeSet<u32>,
    pub word0_active: bool,
    pub word1_active: bool,
    pub oem_state: u8,
    pub gate_suppress: BTreeSet<u32>,
    pub gate_reasons: BTreeMap<u32, serde_json::Value>,
    pub oem_intent: Option<f64>,
    pub timeout: bool,
}
pub struct Provider {
    pub config: Config,
    pub tracker: tracker::Tracker,
    pub camera_extended: camera_extended::Extended,
    pub camera: Option<camera::CameraCycleCache>,
    pub publication_aliases: alias::Allocator,
    pub family_companion: family::Family,
    pub burst_multireturn: burst::Burst,
    pub b5_birth_defer: birth_b5::BirthB5,
    pub mirror_birth_hold: mirror_birth::MirrorBirth,
    pub mirror_m3_shadow: mirror_m3::MirrorM3,
    pub road_edge_filter: road_edge::RoadEdge,
    pub unsupported_birth: unsupported_birth::UnsupportedBirth,
    pub sidepass_lateral: sidepass::SidePass,
    pub p91: persistent_clone::PersistentClone,
    pub oem_gate: oem::Gate,
    pub unsupported_context: Option<unsupported_birth::Context>,
    pub scc_obj_valid: Option<bool>,
    pub scc_obj_valid_ns: i64,
    pub can_error: bool,
    pub wrong_config: bool,
    pub last_scan_timestamp_ns: Option<i64>,
    pub debug: Debug,
    pub frames: Vec<CanFrame>,
    pub anchors: Vec<(i64, u32)>,
    pub order: u64,
    pub start_ns: Option<i64>,
    pub last_now_ns: Option<i64>,
    pub last_closed_anchor_ns: Option<i64>,
    pub last_output_ns: Option<i64>,
    pub pending_error: bool,
    pub test_last_suppressed: Vec<u32>,
    pub test_last_active_groups: usize,
    pub last_oem_state: u8,
    pub companion_pairs: BTreeMap<(i64, u32, u32), (usize, i64)>,
    pub companion_deferred_points: usize,
    pub companion_defer_scans: usize,
    pub last_companion_deferred: Vec<u32>,
    pub companion_ns: Option<i64>,
    pub companion_prev_ns: Option<i64>,
    pub companion_hidden: BTreeMap<u32, u32>,
    pub companion_shared_start: BTreeMap<(i64, u32, u32), i64>,
    pub last_oem_slot: Option<usize>,
    pub oem_nearer_corrections: usize,
    pub oem_nearer_scans: usize,
    pub last_oem_nearer: Vec<u32>,
}
impl Provider {
    pub fn new(config: Config) -> Result<Self, String> {
        let tracker = tracker::Tracker::new(
            tracking::Config::default(),
            group_config::Config {
                provisional_enabled: config.provisional_bundle,
                mature_retention_enabled: config.mature_group_retention,
                ..group_config::Config::default()
            },
        )?;
        Ok(Self {
            camera_extended: camera_extended::Extended::new(
                config.camera_extended_mode,
                config.curve_reacquire_mode,
            )?,
            camera: (config.camera_extended_mode != 0).then(camera::CameraCycleCache::default),
            publication_aliases: alias::Allocator::default(),
            family_companion: family::Family::new(config.family_companion_mode)?,
            burst_multireturn: burst::Burst::new(config.burst_multireturn_mode)?,
            b5_birth_defer: birth_b5::BirthB5::new(config.b5_mode),
            mirror_birth_hold: mirror_birth::MirrorBirth::new(config.mirror_birth_mode)?,
            mirror_m3_shadow: mirror_m3::MirrorM3::new(2)?,
            road_edge_filter: road_edge::RoadEdge::new(true),
            unsupported_birth: unsupported_birth::UnsupportedBirth::new(config.unsupported_birth),
            sidepass_lateral: sidepass::SidePass::new(config.sidepass_lateral_mode)?,
            p91: persistent_clone::PersistentClone::new(config.p91_mode)?,
            oem_gate: oem::Gate::new(config.oem_gate_mode)?,
            config,
            tracker,
            unsupported_context: None,
            scc_obj_valid: None,
            scc_obj_valid_ns: 0,
            can_error: false,
            wrong_config: false,
            last_scan_timestamp_ns: None,
            debug: Debug::default(),
            frames: vec![],
            anchors: vec![],
            order: 0,
            start_ns: None,
            last_now_ns: None,
            last_closed_anchor_ns: None,
            last_output_ns: None,
            pending_error: false,
            test_last_suppressed: vec![],
            test_last_active_groups: 0,
            last_oem_state: 0,
            companion_pairs: BTreeMap::new(),
            companion_deferred_points: 0,
            companion_defer_scans: 0,
            last_companion_deferred: vec![],
            companion_ns: None,
            companion_prev_ns: None,
            companion_hidden: BTreeMap::new(),
            companion_shared_start: BTreeMap::new(),
            last_oem_slot: None,
            oem_nearer_corrections: 0,
            oem_nearer_scans: 0,
            last_oem_nearer: vec![],
        })
    }
    pub fn scc_validity(&self, timestamp: i64) -> Option<bool> {
        self.scc_obj_valid
            .filter(|_| timestamp - self.scc_obj_valid_ns <= 300_000_000)
    }
    fn ingest_scc(&mut self, timestamp: i64, address: u32, payload: &[u8]) {
        if payload.len() < 8 {
            return;
        }
        // SCC accepts extended payloads as the original little-endian integer read did.
        let value = u64::from_le_bytes(payload[..8].try_into().expect("checked SCC length"));
        if address == 0x420 {
            self.scc_obj_valid = Some(value & (1 << 16) != 0);
            self.scc_obj_valid_ns = timestamp;
        } else {
            self.oem_gate.intent.ingest(
                timestamp,
                Some(((value >> 24) & 0x7ff) as f64 * 0.01 - 10.23),
            );
        }
    }
    pub fn update(
        &mut self,
        packets: &[Packet],
        c: Context<'_>,
    ) -> Result<Option<Vec<Object>>, String> {
        if self.last_now_ns.is_some_and(|n| c.now < n) {
            return Err("provider receive clock must not regress".into());
        }
        self.unsupported_context = Some(unsupported_birth::Context(
            c.now,
            birth_b5::Number(c.ego),
            c.yaw.map(birth_b5::Number),
            c.path
                .iter()
                .map(|&(x, y)| (birth_b5::Number(x), birth_b5::Number(y)))
                .collect(),
            c.path_ns,
        ));
        self.last_now_ns = Some(c.now);
        self.start_ns.get_or_insert(c.now);
        for packet in packets {
            let future = packet.timestamp_ns > c.now;
            for m in &packet.messages {
                if m.bus == self.config.camera_bus && (0x738..=0x760).contains(&m.address) {
                    if let Some(camera) = self.camera.as_mut() {
                        if future {
                            camera.fault_ns = packet.timestamp_ns;
                        } else {
                            camera.ingest(packet.timestamp_ns, m.address, &m.payload);
                        }
                        continue;
                    }
                }
                if m.bus != self.config.bus {
                    if m.bus == self.config.scc_bus
                        && (m.address == 0x420 || m.address == 0x421)
                        && !future
                    {
                        self.ingest_scc(packet.timestamp_ns, m.address, &m.payload);
                    }
                    continue;
                }
                if !(0x601..=0x612).contains(&m.address) {
                    continue;
                }
                if future {
                    self.pending_error = true;
                    continue;
                }
                if m.address == 0x612 {
                    if m.payload.len() != 8 {
                        self.pending_error = true;
                        continue;
                    }
                    let tick = u32::from_le_bytes([m.payload[1], m.payload[2], m.payload[3], 0]);
                    if tick.is_multiple_of(10)
                        && self
                            .last_closed_anchor_ns
                            .is_none_or(|n| packet.timestamp_ns > n)
                        && !self.anchors.iter().any(|a| a.0 == packet.timestamp_ns)
                    {
                        self.anchors.push((packet.timestamp_ns, tick));
                    }
                } else {
                    self.frames.push(CanFrame {
                        timestamp_ns: packet.timestamp_ns,
                        address: m.address,
                        payload: m.payload.clone(),
                        order: self.order,
                    });
                    self.order += 1;
                }
            }
        }
        if self.anchors.len() > 1 {
            self.anchors.sort();
        }
        let mut output = None;
        while self
            .anchors
            .first()
            .is_some_and(|a| a.0 + 20_000_000 <= c.now)
        {
            let (phase, tick) = self.anchors[0];
            let mut assigned = vec![];
            let mut retained = vec![];
            for frame in self.frames.drain(..) {
                let closest = self
                    .anchors
                    .iter()
                    .min_by_key(|a| (a.0 - frame.timestamp_ns).abs())
                    .expect("anchor exists")
                    .0;
                if closest == phase && (phase - frame.timestamp_ns).abs() <= 20_000_000 {
                    assigned.push(frame);
                } else {
                    retained.push(frame);
                }
            }
            self.frames = retained;
            self.anchors.remove(0);
            self.last_closed_anchor_ns = Some(phase);
            let path = if c
                .path_ns
                .is_some_and(|ns| (0..=200_000_000).contains(&(c.now - ns)))
                && c.path_source_ns.filter(|n| *n != 0).map_or_else(
                    || {
                        c.path_ns.is_some_and(|ns| {
                            (phase - 200_000_000..=phase + 20_000_000).contains(&ns)
                        })
                    },
                    |ns| (phase - 200_000_000..=phase).contains(&ns),
                ) {
                c.path
            } else {
                &[]
            };
            output = Some(self.finish_scan(phase, tick, &assigned, &c, path)?);
            self.last_output_ns = Some(c.now);
        }
        let oldest = self.anchors.first().map_or(c.now, |a| a.0) - 20_000_000;
        self.frames.retain(|f| f.timestamp_ns >= oldest);
        let since = self
            .last_scan_timestamp_ns
            .or(self.start_ns)
            .expect("start set");
        if c.now - since >= 300_000_000
            && (output.is_some()
                || self
                    .last_output_ns
                    .is_none_or(|ns| c.now - ns >= 100_000_000))
        {
            self.timeout(c.now, c.ego, c.yaw);
            return Ok(Some(vec![]));
        }
        Ok(output)
    }
    fn timeout(&mut self, now: i64, ego: f64, yaw: Option<f64>) {
        self.can_error = true;
        self.debug.objects.clear();
        self.debug.timeout = true;
        self.tracker
            .group_manager
            .reset_provisional(now, "provider_timeout");
        self.tracker.group_manager.ancestry.reset();
        self.companion_pairs.clear();
        self.companion_shared_start.clear();
        self.companion_hidden.clear();
        self.companion_ns = None;
        self.companion_prev_ns = None;
        self.last_companion_deferred.clear();
        self.family_companion.reset(now, "STATE_RESET");
        self.burst_multireturn.reset(now, "STATE_RESET");
        self.b5_birth_defer.reset("PROVIDER_TIMEOUT");
        self.mirror_birth_hold.reset(Some(now), "PROVIDER_TIMEOUT");
        self.mirror_m3_shadow.reset(Some(now), "PROVIDER_TIMEOUT");
        self.road_edge_filter.reset();
        self.sidepass_lateral.reset(now, "PROVIDER_TIMEOUT");
        self.p91.update(&[], now, ego, &BTreeSet::new(), yaw);
        self.oem_gate
            .update(&[], now, ego, 0, &BTreeSet::new(), None);
        self.debug.gate_suppress.clear();
        self.debug.gate_reasons.clear();
        self.last_output_ns = Some(now);
    }
    fn finish_scan(
        &mut self,
        phase: i64,
        tick: u32,
        frames: &[CanFrame],
        c: &Context<'_>,
        path: &[(f64, f64)],
    ) -> Result<Vec<Object>, String> {
        let mut bucket: BTreeMap<u32, &CanFrame> = BTreeMap::new();
        let mut malformed = self.pending_error || frames.iter().any(|f| f.payload.len() != 8);
        self.pending_error = false;
        let mut ordered: Vec<_> = frames.iter().collect();
        ordered.sort_by_key(|f| (f.timestamp_ns, f.order));
        for frame in ordered {
            if frame.payload.len() != 8 {
                continue;
            }
            if bucket.get(&frame.address).is_none_or(|old| {
                (frame.timestamp_ns - phase).abs() < (old.timestamp_ns - phase).abs()
            }) {
                bucket.insert(frame.address, frame);
            }
        }
        let complete = (0x602..0x612).all(|a| bucket.contains_key(&a));
        let mut availability = bucket
            .values()
            .filter(|f| (0x602..0x612).contains(&f.address))
            .map(|f| f.timestamp_ns)
            .fold(phase, i64::max);
        if !complete {
            availability = availability.max(phase + 20_000_000);
        }
        let mut ordered: Vec<_> = bucket.values().copied().collect();
        ordered.sort_by_key(|f| (f.timestamp_ns, f.order));
        let mut detections = vec![];
        let mut unsupported = false;
        for frame in ordered {
            if (0x602..0x612).contains(&frame.address) {
                let (decoded, unknown) =
                    raw::decode_frame(frame.timestamp_ns, frame.address, &frame.payload)?;
                detections.extend(decoded);
                unsupported |= unknown;
            }
        }
        if let Some(previous) = self.tracker.raw_manager.last_timestamp_ns {
            if availability <= previous {
                self.can_error = true;
                self.debug.objects.clear();
                return Ok(vec![]);
            }
            let len = detections.len();
            detections.retain(|d| d.timestamp_ns > previous);
            malformed |= len != detections.len();
        }
        let oem = bucket.get(&0x601);
        let oem_word = oem
            .map(|f| u32::from_le_bytes(f.payload[4..8].try_into().expect("checked OEM length")));
        let processed =
            oem.map(|f| u32::from_le_bytes(f.payload[..4].try_into().expect("checked OEM length")));
        let matches: Vec<_> = detections
            .iter()
            .filter(|d| Some(d.raw_word) == oem_word)
            .map(|d| d.slot)
            .collect();
        let oem_slot = matches.first().copied();
        let active = |w: Option<u32>| w.is_some_and(|w| w != 0x4010_0000 && w & (1 << 31) == 0);
        let word0 = active(processed);
        let word1 = active(oem_word);
        let oem_state = oem::Gate::classify(word0, word1);
        if let Some(retention) = &mut self.tracker.group_manager.mature_retention {
            retention.ingest_path(path, availability);
        }
        let objects =
            self.tracker
                .update(availability, &detections, c.yaw, c.ego, oem_slot, c.vision)?;
        self.last_scan_timestamp_ns = Some(availability);
        self.can_error = !complete || malformed || unsupported;
        self.wrong_config = unsupported;
        self.debug.objects = objects.clone();
        self.debug.phase_ns = Some(phase);
        self.debug.closed_ns = self.last_now_ns;
        self.debug.tick = Some(tick);
        self.debug.complete = complete;
        self.debug.oem_word = oem_word;
        self.debug.oem_slot = oem_slot;
        self.debug.oem_matches = matches.len();
        self.debug.processed_word = processed;
        self.debug.timeout = false;
        let observed: Vec<_> = objects.iter().map(camera_extended::Object::from).collect();
        let snapshot = self
            .camera
            .as_ref()
            .and_then(|camera| camera.snapshot(availability))
            .map(
                |(objects, count, _, complete_ns)| camera_extended::Snapshot {
                    objects,
                    count,
                    complete_ns,
                },
            );
        self.camera_extended
            .update(availability, &observed, c.ego, c.yaw, snapshot)?;
        // The final source qualifier is an immutable pass-through in both configurations.
        let processed_pids = points::match_processed(&objects, processed);
        let valid = self.scc_validity(availability);
        self.debug.processed_pids = processed_pids.clone();
        self.debug.word0_active = word0;
        self.debug.word1_active = word1;
        self.debug.oem_state = oem_state;
        self.last_oem_state = oem_state;
        self.last_oem_slot = oem_slot;
        let associations: BTreeMap<_, _> = self
            .camera_extended
            .last_associations
            .iter()
            .map(|(&pid, &(verdict, episode, _))| (pid, (verdict, episode)))
            .collect();
        let strict: BTreeMap<_, _> = associations
            .iter()
            .filter(|(_, v)| v.0 == 1 && v.1 >= 0)
            .map(|(&pid, v)| (pid, v.1 as usize))
            .collect();
        let mut family_oem = processed_pids.clone();
        family_oem.extend(
            objects
                .iter()
                .filter(|o| o.oem_selected)
                .map(|o| o.physical_track_id),
        );
        let excluded = self
            .tracker
            .group_manager
            .provisional_pid_pairs
            .values()
            .flat_map(|&(a, b)| [a, b])
            .collect();
        self.family_companion.update(family::Frame {
            objects: &objects,
            timestamp: availability,
            ego: c.ego,
            yaw: c.yaw,
            strict: &strict,
            oem: &family_oem,
            excluded: &excluded,
            path,
        });
        let empty = BTreeSet::new();
        let validated = if oem_state == 3 {
            &processed_pids
        } else {
            &empty
        };
        self.burst_multireturn
            .update(&objects, availability, c.ego, c.yaw, validated);
        self.oem_gate.update(
            &objects,
            availability,
            c.ego,
            oem_state,
            &processed_pids,
            valid,
        );
        self.debug.gate_suppress = self.oem_gate.would_withhold.clone();
        self.debug.gate_reasons = self.oem_gate.reasons.clone();
        self.debug.oem_intent = self.oem_gate.intent.median(availability);
        self.p91.update(
            &objects,
            availability,
            c.ego,
            if valid != Some(false) {
                validated
            } else {
                &empty
            },
            c.yaw,
        );
        let live = self.tracker.group_manager.states.keys().copied().collect();
        let live_raw = self.tracker.raw_manager.states.keys().copied().collect();
        self.b5_birth_defer.update(birth_b5::Frame {
            objects: &objects,
            raw: &self.tracker.last_raw_tracks,
            scan: availability,
            now: self.last_now_ns.expect("receive clock set"),
            ego: c.ego,
            yaw: c.yaw,
            camera: &associations,
            oem_state,
            word0: &processed_pids,
            live_pids: &live,
            live_raw: &live_raw,
        })?;
        self.mirror_birth_hold.update(mirror_birth::Frame {
            objects: &objects,
            raw: &self.tracker.last_raw_tracks,
            timestamp: availability,
            ego: c.ego,
            yaw: c.yaw,
            camera: &associations,
            word0: &processed_pids,
            live: Some(&live),
        })?;
        let mirror = self
            .mirror_birth_hold
            .last_mirror_objects
            .iter()
            .map(|(&pid, v)| (pid, (v.0, v.1, v.2, v.3, v.4.clone(), Some(v.5), v.6, v.7)))
            .collect();
        let decisions = self
            .mirror_birth_hold
            .last_decisions_by_pid
            .iter()
            .map(|(&pid, v)| {
                (
                    pid,
                    mirror_m3::BirthDecision {
                        action: v.action.clone(),
                        parent_pid: v.parent_pid,
                        wall_y_m: v.wall_y_m,
                        residual_m: v.residual_m,
                    },
                )
            })
            .collect();
        self.mirror_m3_shadow.update(mirror_m3::Frame {
            timestamp: availability,
            ego: c.ego,
            mirror: &mirror,
            decisions: &decisions,
            camera: &associations,
            word0: &processed_pids,
            mirror_enabled: self.mirror_birth_hold.mode != 0,
            live: Some(&live),
        });
        let scan_objects: Vec<_> = objects.iter().map(road_edge::ScanObject::from).collect();
        self.road_edge_filter.update(
            &scan_objects,
            availability,
            !self.can_error && !self.wrong_config,
            &associations,
            &processed_pids,
        );
        self.sidepass_lateral
            .update(&objects, availability, c.ego, path, validated)?;
        Ok(objects)
    }

    fn companion_pairs_update(&mut self, objects: &[Object]) -> BTreeMap<u32, u32> {
        let ext = &self.camera_extended;
        let timestamp = ext.last_ns.expect("extended scan set");
        if self
            .companion_prev_ns
            .is_some_and(|last| timestamp - last > 160_000_000)
        {
            self.companion_pairs.clear();
            self.companion_shared_start.clear();
        }
        self.companion_prev_ns = Some(timestamp);
        if ext
            .last_camera_ns
            .is_none_or(|ns| !(0..=160_000_000).contains(&(timestamp - ns)))
        {
            self.companion_pairs.clear();
            self.companion_shared_start.clear();
            return BTreeMap::new();
        }
        let by_pid: BTreeMap<_, _> = objects
            .iter()
            .filter(|o| o.timestamp_ns == timestamp)
            .map(|o| (o.physical_track_id, o))
            .collect();
        let mut episodes: BTreeMap<i64, Vec<u32>> = BTreeMap::new();
        for (&pid, &(verdict, episode, _)) in &ext.last_associations {
            if verdict == 1 && episode >= 0 && by_pid.contains_key(&pid) {
                episodes.entry(episode).or_default().push(pid);
            }
        }
        let validated = self.last_oem_state == 3;
        let ego = ext.last_v_ego.0;
        let yaw = ext
            .last_yaw_rate
            .map(|v| v.0)
            .filter(|v| v.is_finite())
            .unwrap_or(0.);
        let mut pairs = BTreeMap::new();
        let mut starts = BTreeMap::new();
        let mut hidden = BTreeMap::new();
        for (episode, mut pids) in episodes {
            if pids.len() < 2 {
                continue;
            }
            let Some(camera) = ext.last_camera_by_episode.get(&episode) else {
                continue;
            };
            let limit = if camera.width_m < 2. {
                5.
            } else if camera.width_m < 2.3 {
                7.
            } else {
                12.
            };
            pids.sort_by(|a, b| {
                by_pid[a]
                    .d_rel
                    .partial_cmp(&by_pid[b].d_rel)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.cmp(b))
            });
            for pair in pids.windows(2) {
                let near = by_pid[&pair[0]];
                let far = by_pid[&pair[1]];
                let key = (episode, pair[0], pair[1]);
                let (mut confirmations, mut last) =
                    self.companion_pairs.get(&key).copied().unwrap_or((0, 0));
                let start = self
                    .companion_shared_start
                    .get(&key)
                    .copied()
                    .unwrap_or(timestamp);
                let conflict = if ego.is_finite() {
                    let a = (near.v_rel + ego - yaw * near.y_rel).abs();
                    let b = (far.v_rel + ego - yaw * far.y_rel).abs();
                    a.min(b) <= 0.6 && a.max(b) >= 1.4
                } else {
                    false
                };
                let baseline = validated
                    && near.oem_selected
                    && !far.oem_selected
                    && !conflict
                    && far.d_rel - near.d_rel > 3.
                    && far.d_rel - near.d_rel <= limit
                    && (far.y_rel - near.y_rel).abs() <= 1.5
                    && (far.v_rel - near.v_rel).abs() <= 1.
                    && near.y_rel.abs() <= 2.5
                    && far.y_rel.abs() <= 2.5;
                let (split, handoff) = self
                    .tracker
                    .group_manager
                    .ancestry
                    .evidence(far, near, start, timestamp);
                let common = split && handoff;
                if baseline && common {
                    confirmations += 1;
                    last = timestamp;
                } else if !(common
                    && confirmations >= 1
                    && (0..=200_000_000).contains(&(timestamp - last)))
                {
                    confirmations = 0;
                }
                if pairs.len() < 16 {
                    pairs.insert(key, (confirmations, last));
                    starts.insert(key, start);
                }
                if confirmations >= 1 {
                    hidden.insert(pair[1], pair[0]);
                }
            }
        }
        self.companion_pairs = pairs;
        self.companion_shared_start = starts;
        hidden
    }
    fn companion_view(&mut self, objects: &[Object]) -> Vec<Object> {
        if self.camera_extended.mode == 0
            || self.camera_extended.last_ns.is_none()
            || objects.is_empty()
        {
            self.last_companion_deferred.clear();
            return objects.to_vec();
        }
        if self.camera_extended.last_ns != self.companion_ns {
            self.companion_ns = self.camera_extended.last_ns;
            self.companion_hidden = self.companion_pairs_update(objects);
        }
        let present: BTreeSet<_> = objects.iter().map(|o| o.physical_track_id).collect();
        let deferred: BTreeSet<_> = self
            .companion_hidden
            .iter()
            .filter(|(far, near)| present.contains(far) && present.contains(near))
            .map(|(&far, _)| far)
            .collect();
        self.last_companion_deferred = deferred.iter().copied().collect();
        if !deferred.is_empty() {
            self.companion_deferred_points += deferred.len();
            self.companion_defer_scans += 1;
        }
        objects
            .iter()
            .filter(|o| !deferred.contains(&o.physical_track_id))
            .cloned()
            .collect()
    }
    fn oem_nearer_view(&mut self, objects: Vec<Object>) -> Vec<Object> {
        self.last_oem_nearer.clear();
        let (Some(slot), Some(scan)) = (self.last_oem_slot, self.last_scan_timestamp_ns) else {
            return objects;
        };
        let output = objects
            .into_iter()
            .map(|mut o| {
                if o.members.len() >= 2 && o.timestamp_ns == scan {
                    if let Some(owned) = o.members.iter().find(|m| m.detection.slot == slot) {
                        if owned.detection.d_rel < o.d_rel {
                            o.published_surface = Some(Surface {
                                raw_track_id: owned.raw_track_id,
                                d_rel: owned.detection.d_rel,
                                y_rel: owned.detection.y_rel,
                                v_rel: owned.detection.v_rel,
                                lateral_estimate: false,
                            });
                            self.last_oem_nearer.push(o.physical_track_id);
                        }
                    }
                }
                o
            })
            .collect();
        if !self.last_oem_nearer.is_empty() {
            self.oem_nearer_corrections += self.last_oem_nearer.len();
            self.oem_nearer_scans += 1;
        }
        output
    }
    fn publication_without_road(&mut self, objects: &[Object]) -> Vec<Object> {
        let strict = if objects
            .first()
            .is_some_and(|o| Some(o.timestamp_ns) == self.camera_extended.last_ns)
        {
            self.camera_extended
                .last_associations
                .iter()
                .filter(|(_, v)| v.0 == 1 && v.1 >= 0)
                .map(|(&pid, v)| (pid, v.1 as usize))
                .collect()
        } else {
            BTreeMap::new()
        };
        let mut oem = self.debug.processed_pids.clone();
        oem.extend(
            objects
                .iter()
                .filter(|o| o.oem_selected)
                .map(|o| o.physical_track_id),
        );
        let mut objects = self
            .tracker
            .group_manager
            .provisional_publication_view(objects, &strict, &oem);
        if self.p91.mode == 2
            && !objects.is_empty()
            && objects
                .iter()
                .all(|o| Some(o.timestamp_ns) == self.p91.last_ns)
        {
            self.p91.publication_suppressed += self.p91.would_suppress.len();
            objects.retain(|o| !self.p91.would_suppress.contains(&o.physical_track_id));
        }
        if self.oem_gate.mode == 2
            && !objects.is_empty()
            && objects
                .iter()
                .all(|o| Some(o.timestamp_ns) == self.oem_gate.last_ns)
        {
            self.oem_gate.publication_withheld += self.oem_gate.would_withhold.len();
            objects.retain(|o| !self.oem_gate.would_withhold.contains(&o.physical_track_id));
        }
        let ext = &self.camera_extended;
        if ext.mode == 3 {
            let mut suppressed = BTreeSet::new();
            let mut active = 0;
            if !ext.mature_groups.is_empty() && !objects.is_empty() {
                let by_pid: BTreeMap<_, _> =
                    objects.iter().map(|o| (o.physical_track_id, o)).collect();
                for representative in &ext.representatives {
                    let members: Vec<_> = representative.members.iter().copied().collect();
                    if ext.mature_groups.contains(&members)
                        && members.iter().all(|p| {
                            by_pid
                                .get(p)
                                .is_some_and(|o| Some(o.timestamp_ns) == ext.last_ns)
                        })
                    {
                        suppressed.extend(
                            members
                                .into_iter()
                                .filter(|p| *p != representative.representative_pid),
                        );
                        active += 1;
                    }
                }
            }
            self.test_last_suppressed = suppressed.iter().copied().collect();
            self.test_last_active_groups = active;
            objects.retain(|o| !suppressed.contains(&o.physical_track_id));
        }
        objects = self.companion_view(&objects);
        objects = self.oem_nearer_view(objects);
        objects = self.family_companion.publication_view(&objects);
        if !objects.is_empty()
            && objects
                .iter()
                .all(|o| Some(o.timestamp_ns) == self.burst_multireturn.last_ns)
        {
            objects = self.burst_multireturn.publication_view(&objects);
        }
        objects = self.b5_birth_defer.publication_view(&objects);
        objects = self.mirror_birth_hold.publication_view(&objects);
        objects = self.mirror_m3_shadow.publication_view(&objects);
        self.sidepass_lateral.publication_view(&objects)
    }
    pub fn publication_view(
        &mut self,
        objects: &[Object],
        timestamp: Option<i64>,
    ) -> Result<Vec<Object>, String> {
        let objects = self.publication_without_road(objects);
        let objects = self.road_edge_filter.publication_view(&objects, timestamp);
        // The final pin disables the unsupported-first-birth publication veto. Its
        // verdict remains available to the enabled native-surface corner gate.
        let snapshot = self
            .last_scan_timestamp_ns
            .and_then(|ns| self.camera.as_ref().and_then(|camera| camera.snapshot(ns)))
            .map(
                |(objects, count, _, complete)| unsupported_birth::Snapshot {
                    objects,
                    count,
                    complete,
                },
            );
        let associations = self
            .camera_extended
            .last_associations
            .iter()
            .map(|(&pid, &v)| (pid, v))
            .collect();
        let frame = unsupported_birth::Frame {
            can_error: self.can_error,
            wrong_config: self.wrong_config,
            context: self.unsupported_context.as_ref(),
            scan: self.last_scan_timestamp_ns,
            identity: &self.tracker.raw_manager.last_identity_evidence,
            extended_ns: self.camera_extended.last_ns,
            associations: &associations,
            processed: &self.debug.processed_pids,
            snapshot,
        };
        self.road_edge_filter
            .publication_surface_view(&frame, &self.unsupported_birth, &objects, timestamp)
            .map_err(str::to_owned)
    }

    pub fn slot_to_ids(&self) -> BTreeMap<usize, (u32, u32)> {
        self.debug
            .objects
            .iter()
            .flat_map(|o| {
                o.members
                    .iter()
                    .map(move |m| (m.detection.slot, (m.raw_track_id, o.physical_track_id)))
            })
            .collect()
    }
    pub fn debug_snapshot(&self) -> serde_json::Value {
        use serde_json::json;
        if self.debug.timeout {
            return json!({"bus":self.config.bus,"timeout":true,"last_scan_timestamp_ns":self.last_scan_timestamp_ns,"objects":[],"slot_to_ids":{}});
        }
        if self.debug.phase_ns.is_none() {
            return json!({});
        }
        let mut result = serde_json::Map::new();
        result.extend(json!({
         "bus":self.config.bus,"phase_ns":self.debug.phase_ns,"scan_timestamp_ns":self.last_scan_timestamp_ns,"closed_ns":self.debug.closed_ns,"tick":self.debug.tick,"complete":self.debug.complete,"can_error":self.can_error,"wrong_config":self.wrong_config,
         "oem_word":self.debug.oem_word,"oem_selected_slot":self.debug.oem_slot,"oem_match_count":self.debug.oem_matches,"slot_to_ids":self.slot_to_ids(),"processed_target_word":self.debug.processed_word,"processed_target_pids":self.debug.processed_pids,
         "p91_mode":self.p91.mode,"p91_would_suppress":self.p91.would_suppress,"p91_decisions":self.p91.decisions,
        }).as_object().expect("static debug map").clone());
        result.extend(json!({
         "oem_state":self.debug.oem_state,"oem_word0_active":self.debug.word0_active,"oem_word1_active":self.debug.word1_active,"scc_obj_valid":self.scc_validity(self.last_scan_timestamp_ns.unwrap_or(0)),"oem_intent_mps2":self.debug.oem_intent,"oem_gate_mode":self.oem_gate.mode,"oem_gate_withhold":self.debug.gate_suppress,"oem_gate_reasons":self.debug.gate_reasons,
         "curve_reacquire_mode":self.camera_extended.curve_reacquire_mode,"curve_reacquire":self.camera_extended.last_curve_reacquire,"curve_reacquire_would":self.camera_extended.last_curve_reacquire_would,"curve_reacquire_state_count":self.camera_extended.curve_reacquire_histories.len(),
         "provisional_hidden":self.tracker.group_manager.provisional_hidden,"provisional_decisions":self.tracker.group_manager.last_provisional_decisions,"family_companion_suppressed":self.family_companion.would_suppress,"family_companion_decisions":self.family_companion.last_decisions,
        }).as_object().expect("static debug map").clone());
        result.extend(json!({
         "burst_multireturn_mode":self.burst_multireturn.mode,"burst_multireturn_suppressed":self.burst_multireturn.would_suppress,"burst_multireturn_decisions":self.burst_multireturn.last_decisions,"b5_mode":self.b5_birth_defer.mode,"b5_suppressed":self.b5_birth_defer.would_suppress,"b5_decisions":self.b5_birth_defer.last_decisions,
         "sidepass_lateral_mode":self.sidepass_lateral.mode,"sidepass_lateral_published_y":self.sidepass_lateral.published_y,"sidepass_lateral_decisions":self.sidepass_lateral.last_decisions,
         "objects":self.debug.objects.iter().map(|o|json!({"physicalTrackId":o.physical_track_id,"rawTrackIds":o.members.iter().map(|m|m.raw_track_id).collect::<Vec<_>>(),"slots":o.members.iter().map(|m|m.detection.slot).collect::<Vec<_>>(),"measurement_ns":o.members.iter().map(|m|m.detection.timestamp_ns).collect::<Vec<_>>(),"representative_rawTrackId":o.representative_raw_track_id,"representative_slot":o.members.iter().find(|m|m.raw_track_id==o.representative_raw_track_id).map(|m|m.detection.slot),"oem_selected":o.oem_selected})).collect::<Vec<_>>()
        }).as_object().expect("static debug map").clone());
        serde_json::Value::Object(result)
    }
}
