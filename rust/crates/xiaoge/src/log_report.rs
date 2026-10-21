//! Offline observations from full-cereal rlogs. This is not an acceptance/safety gate.
//!
//! Resource mode_since_ns defines mode acceptance; legacy markers use Event.logMonoTime.
//! Structured telemetry uses its source mono_ns; full-cereal services use Event time.
//! Aggregate backend snapshots are never backdated to mode acceptance. Exact nearest-rank
//! quantiles retain numeric samples, not messages. Two passes retain only metadata,
//! mode markers and those numeric samples. Multiple inputs require matching explicit
//! boot identity (or an exact cached logger InitData clock pair) and disjoint time
//! ranges; ambiguous reboot/overlap is rejected. Cached InitData and segment
//! sentinels (which share boundary timestamps) are not data range edges.
use openpilot_cereal::log_capnp::event;
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    error::Error,
    fs::File,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const MAX_JSON_LINE: u64 = 1024 * 1024;
const SERVICES: &[&str] = &[
    "modelV2",
    "driverStateV2",
    "roadCameraState",
    "wideRoadCameraState",
    "driverCameraState",
    "deviceState",
    "xiaoge_inference",
    "xiaoge_road",
    "xiaoge_wide",
    "xiaoge_resources",
    "xiaoge_backend",
];
const METRICS: &[&str] = &[
    "driving_execution_ms",
    "driving_frame_drop_percent",
    "dm_execution_ms",
    "dm_gpu_execution_ms",
    "road_sof_interval_ms",
    "wide_sof_interval_ms",
    "driver_sof_interval_ms",
    "xiaoge_latency_ms",
    "xiaoge_thread_cpu_ms",
    "xiaoge_queue_wait_ms",
    "xiaoge_primary_ms",
    "xiaoge_primary_thread_cpu_ms",
    "xiaoge_compare_wall_ms",
    "xiaoge_compare_thread_cpu_ms",
    "process_cpu_percent",
    "process_rss_mib",
    "internal_gpu_busy_percent",
    "comparison_max_abs",
    "comparison_mean_abs",
    "comparison_max_rel",
    "device_gpu_usage_percent",
    "device_cpu_usage_percent",
    "device_cpu_temp_c",
    "device_gpu_temp_c",
];

#[derive(Debug, Default, Serialize)]
pub struct Summary {
    pub sample_scope: &'static str,
    pub count: usize,
    pub rejected: u64,
    pub mean: Option<f64>,
    pub p50: Option<f64>,
    pub p95: Option<f64>,
    pub p99: Option<f64>,
    pub max: Option<f64>,
}
#[derive(Debug, Default, Serialize)]
pub struct ServiceStats {
    pub count: u64,
    pub invalid: u64,
    /// Observed ID discontinuities, not proof of skipped inference (DM may decimate).
    pub frame_id_missing: u64,
    pub frame_id_gap_events: u64,
    pub frame_id_repeats: u64,
    pub frame_id_regressions: u64,
    pub timestamp_regressions: u64,
    pub missing_sof: u64,
    pub sof_gaps_over_75ms: u64,
}
#[derive(Debug, Default, Serialize)]
pub struct ComparisonStats {
    pub records: u64,
    pub missing_records: u64,
    /// Records with at least one tensor observation, not a complete check/pass count.
    pub tensor_checked: u64,
    /// Exact inequality, not a tolerance-based failure decision.
    pub tensor_unequal_records: u64,
    pub shape_mismatches: u64,
    pub shape_checked: u64,
    pub bits_checked: u64,
    pub nonfinite_checked: u64,
    pub unequal_bits: u64,
    pub nonfinite_elements: u64,
    pub decoded_checked: u64,
    pub decoded_mismatches: u64,
    /// Only explicit decision_equal booleans are compared; zero means unobserved.
    pub decisions_compared: u64,
    pub decision_mismatches: u64,
    pub errors: u64,
}
#[derive(Debug, Serialize)]
pub struct Group {
    pub mode: Option<u8>,
    pub effective_backend: String,
    pub diagnostic_only: bool,
    pub services: BTreeMap<String, ServiceStats>,
    pub metrics: BTreeMap<String, Summary>,
    pub comparison: ComparisonStats,
    pub mode_conflicts: u64,
    pub sampled_inference_records: u64,
    pub usb_gpu_active_true: u64,
    pub usb_gpu_active_false: u64,
}
#[derive(Debug, Serialize)]
pub struct Report {
    pub schema: u8,
    pub input_format: &'static str,
    pub inputs: Vec<InputInfo>,
    pub mode_markers: usize,
    pub physical_device_execution: &'static str,
    pub groups: Vec<Group>,
    pub limitations: Vec<&'static str>,
}
#[derive(Debug, Default, Serialize)]
pub struct InputInfo {
    pub path: PathBuf,
    pub bootlog_id: Option<String>,
    /// [InitData Event.logMonoTime, InitData.wallTimeNanos]; identifies cached headers.
    pub logger_init_clock: Option<[u64; 2]>,
    pub first_mono_ns: Option<u64>,
    pub last_mono_ns: Option<u64>,
    pub events: u64,
}
impl InputInfo {
    fn observe(&mut self, mono: u64) -> Result<()> {
        if self
            .last_mono_ns
            .is_some_and(|last| mono.saturating_add(1_000_000_000) < last)
        {
            return Err(
                "logMonoTime regressed by over one second; possible reboot, split inputs".into(),
            );
        }
        self.first_mono_ns = Some(self.first_mono_ns.map_or(mono, |v| v.min(mono)));
        self.last_mono_ns = Some(self.last_mono_ns.map_or(mono, |v| v.max(mono)));
        self.events += 1;
        Ok(())
    }
    fn boot(&mut self, id: &str) -> Result<()> {
        if id.is_empty() {
            return Ok(());
        }
        if self.bootlog_id.as_deref().is_some_and(|old| old != id) {
            return Err("multiple bootlogId values; split reboot timelines".into());
        }
        self.bootlog_id = Some(id.to_owned());
        Ok(())
    }
}
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    mode: Option<u8>,
    backend: String,
}
impl Key {
    fn unknown() -> Self {
        Self {
            mode: None,
            backend: "unknown".into(),
        }
    }
    fn from_payload(value: &Value) -> Result<Self> {
        let mode = match value["mode"].as_u64() {
            Some(v @ 0..=2) => v as u8,
            _ => return Err("Xiaoge telemetry requires mode=0/1/2".into()),
        };
        let backend = match value["effective_backend"].as_str() {
            Some(v @ ("cpu" | "opencl-fp32")) => v,
            Some("mixed") if value["event"] == "xiaoge_resources" => "mixed",
            Some("pending") => "unknown",
            None if value["event"] == "xiaoge_resources"
                && value["effective_backend"].is_null() =>
            {
                "unknown"
            }
            _ => return Err("Xiaoge effective_backend requires cpu/opencl-fp32/pending; resources also permit mixed or missing".into()),
        };
        Ok(Self {
            mode: Some(mode),
            backend: backend.into(),
        })
    }
}
#[derive(Default)]
struct Samples {
    values: Vec<f64>,
    rejected: u64,
}
impl Samples {
    fn add(&mut self, value: f64) {
        if value.is_finite() && value >= 0.0 {
            self.values.push(value);
        } else {
            self.rejected += 1;
        }
    }
    fn finish(mut self, sample_scope: &'static str) -> Summary {
        self.values.sort_unstable_by(f64::total_cmp);
        let count = self.values.len();
        let quantile = |percent: usize| {
            if count == 0 {
                None
            } else {
                Some(self.values[(count * percent).div_ceil(100) - 1])
            }
        };
        Summary {
            sample_scope,
            count,
            rejected: self.rejected,
            mean: (count != 0).then(|| self.values.iter().map(|v| v / count as f64).sum()),
            p50: quantile(50),
            p95: quantile(95),
            p99: quantile(99),
            max: self.values.last().copied(),
        }
    }
}
struct Accumulator {
    group: Group,
    samples: BTreeMap<String, Samples>,
}
impl Accumulator {
    fn new(key: &Key) -> Self {
        Self {
            group: Group {
                mode: key.mode,
                effective_backend: key.backend.clone(),
                diagnostic_only: !matches!(key.mode, Some(0 | 1))
                    || matches!(key.backend.as_str(), "unknown" | "mixed"),
                services: SERVICES
                    .iter()
                    .map(|name| ((*name).into(), ServiceStats::default()))
                    .collect(),
                metrics: BTreeMap::new(),
                comparison: ComparisonStats::default(),
                mode_conflicts: 0,
                sampled_inference_records: 0,
                usb_gpu_active_true: 0,
                usb_gpu_active_false: 0,
            },
            samples: METRICS
                .iter()
                .map(|name| ((*name).into(), Samples::default()))
                .collect(),
        }
    }
    fn metric(&mut self, name: &str, value: f64) {
        self.samples.entry(name.into()).or_default().add(value);
    }
    fn json_metric(&mut self, name: &str, value: &Value) {
        if value.is_null() {
            return;
        }
        if let Some(value) = value.as_f64() {
            self.metric(name, value);
        } else {
            self.samples.entry(name.into()).or_default().rejected += 1;
        }
    }
    fn finish(mut self) -> Group {
        self.group.metrics = self
            .samples
            .into_iter()
            .map(|(k, v)| {
                let scope = if k.starts_with("xiaoge_") || k.starts_with("comparison_") {
                    "sampled Xiaoge inference telemetry"
                } else if matches!(
                    k.as_str(),
                    "process_cpu_percent" | "process_rss_mib" | "internal_gpu_busy_percent"
                ) {
                    "Xiaoge resource snapshots"
                } else {
                    "all available full-cereal events; unsampled by this reader"
                };
                (k, v.finish(scope))
            })
            .collect();
        self.group
    }
}
#[derive(Default)]
struct Timeline {
    // (observation/acceptance time, key, acceptance time). Backend snapshots
    // retain their mode epoch so delayed records cannot leak across re-entry.
    markers: Vec<(u64, Key, u64)>,
    backends: Vec<(u64, Key, u64)>,
}
impl Timeline {
    fn collect(&mut self, mono: u64, value: &Value, valid: bool) -> Result<()> {
        if value["schema"].as_u64() != Some(1) || value["mono_ns"].as_u64().is_none() {
            return Err("Xiaoge telemetry requires schema=1 and integer mono_ns".into());
        }
        let key = Key::from_payload(value)?;
        if value["event"] == "xiaoge_resources" {
            let source_mono = value["mono_ns"].as_u64().ok_or("missing mono_ns")?;
            let since = value
                .get("mode_since_ns")
                .map(|value| {
                    value.as_u64().filter(|since| *since <= source_mono).ok_or(
                        "mode_since_ns must be an integer u64 <= mono_ns in the source boot clock",
                    )
                })
                .transpose()?;
            if valid {
                let anchor = since.unwrap_or(mono);
                self.markers.push((
                    anchor,
                    Key {
                        mode: key.mode,
                        backend: "unknown".into(),
                    },
                    anchor,
                ));
                self.backends.push((
                    if since.is_some() { source_mono } else { mono },
                    key,
                    anchor,
                ));
            }
        }
        Ok(())
    }
    fn finish(&mut self) -> Result<()> {
        for markers in [&mut self.markers, &mut self.backends] {
            markers.sort_unstable();
            for pair in markers.windows(2) {
                if pair[0].0 == pair[1].0 && (pair[0].1 != pair[1].1 || pair[0].2 != pair[1].2) {
                    return Err(
                        "conflicting resource mode/backend markers at the same boot timestamp"
                            .into(),
                    );
                }
            }
            markers.dedup();
        }
        Ok(())
    }
    fn key(&self, mono: u64) -> Key {
        let index = self.markers.partition_point(|(time, _, _)| *time <= mono);
        if index == 0 {
            Key::unknown()
        } else {
            let (accepted, mode, _) = &self.markers[index - 1];
            let backend_index = self.backends.partition_point(|(time, _, _)| *time <= mono);
            if backend_index > 0 {
                let (observed, backend, backend_accepted) = &self.backends[backend_index - 1];
                if observed >= accepted && backend_accepted == accepted && backend.mode == mode.mode
                {
                    return backend.clone();
                }
            }
            mode.clone()
        }
    }
    fn continuous(&self, first: u64, last: u64, key: &Key) -> bool {
        if last < first {
            return false;
        }
        let start = self.markers.partition_point(|(time, _, _)| *time <= first);
        let end = self.markers.partition_point(|(time, _, _)| *time <= last);
        self.markers[start..end]
            .iter()
            .all(|(_, mode, _)| mode.mode == key.mode)
    }
}
struct Previous {
    mono: u64,
    frame: Option<u32>,
    sof: Option<u64>,
    key: Key,
}
#[derive(Default)]
struct FrameMetadata {
    id: Option<u32>,
    sof: Option<u64>,
    sof_metric: Option<&'static str>,
}
struct Aggregator {
    timeline: Timeline,
    groups: BTreeMap<Key, Accumulator>,
    previous: BTreeMap<String, Previous>,
}
impl Aggregator {
    fn new(timeline: Timeline) -> Self {
        Self {
            timeline,
            groups: BTreeMap::new(),
            previous: BTreeMap::new(),
        }
    }
    fn group(&mut self, key: &Key) -> &mut Accumulator {
        self.groups
            .entry(key.clone())
            .or_insert_with(|| Accumulator::new(key))
    }
    fn observe(
        &mut self,
        key: &Key,
        service: &str,
        mono: u64,
        valid: bool,
        metadata: FrameMetadata,
    ) {
        let FrameMetadata {
            id: frame,
            sof,
            sof_metric,
        } = metadata;
        let previous = self.previous.insert(
            service.into(),
            Previous {
                mono,
                frame,
                sof,
                key: key.clone(),
            },
        );
        let continuous = previous
            .as_ref()
            .is_some_and(|p| self.timeline.continuous(p.mono, mono, key));
        let group = self.group(key);
        let stats = group.group.services.entry(service.into()).or_default();
        stats.count += 1;
        stats.invalid += u64::from(!valid);
        if sof == Some(0) {
            stats.missing_sof += 1;
        }
        if let Some(previous) = previous.filter(|p| &p.key == key) {
            if mono < previous.mono {
                stats.timestamp_regressions += 1;
            }
            if !continuous {
                return;
            }
            if let (Some(old), Some(new)) = (previous.frame, frame) {
                if new == old {
                    stats.frame_id_repeats += 1;
                } else if new < old {
                    stats.frame_id_regressions += 1;
                } else if new - old > 1 {
                    stats.frame_id_gap_events += 1;
                    stats.frame_id_missing += u64::from(new - old - 1);
                }
            }
            if let (Some(old), Some(new), Some(metric)) = (previous.sof, sof, sof_metric) {
                if old != 0 && new > old {
                    let delta = new - old;
                    stats.sof_gaps_over_75ms += u64::from(delta > 75_000_000);
                    group.metric(metric, delta as f64 / 1e6);
                }
            }
        }
    }
    fn telemetry(&mut self, mono: u64, valid: bool, value: &Value) -> Result<()> {
        let mono = if value["event"] == "xiaoge_resources" && value.get("mode_since_ns").is_none() {
            mono // Legacy mode markers and their resource sample use the transport clock.
        } else {
            value["mono_ns"]
                .as_u64()
                .ok_or("missing telemetry mono_ns")?
        };
        let recorded_key = Key::from_payload(value)?;
        let mut key = self.timeline.key(mono);
        let conflict = key.mode.is_some() && key.mode != recorded_key.mode;
        if conflict {
            key = Key::unknown();
        } else if key.mode.is_some() {
            key = recorded_key.clone();
        }
        let name = value["event"].as_str().ok_or("missing telemetry event")?;
        self.observe(&key, name, mono, valid, FrameMetadata::default());
        let group = self.group(&key);
        group.group.mode_conflicts += u64::from(conflict);
        match name {
            "xiaoge_resources" => {
                // Support an Object named resources and flat records from early emitters.
                let resources = value.get("resources").unwrap_or(value);
                group.json_metric("process_cpu_percent", &resources["process_cpu_percent"]);
                if let Some(bytes) = resources["process_rss_bytes"].as_u64() {
                    group.metric("process_rss_mib", bytes as f64 / 1_048_576.0);
                }
                if resources["internal_gpu_counter_valid"].as_bool() == Some(true) {
                    group.json_metric(
                        "internal_gpu_busy_percent",
                        &resources["internal_gpu_busy_percent"],
                    );
                }
                if let Some(selected) = value
                    .get("usb_gpu_active")
                    .or_else(|| value.get("UsbGpuActive"))
                    .or_else(|| resources.get("usb_gpu_active"))
                    .or_else(|| resources.get("UsbGpuActive"))
                    .and_then(Value::as_bool)
                {
                    group.group.usb_gpu_active_true += u64::from(selected);
                    group.group.usb_gpu_active_false += u64::from(!selected);
                }
            }
            "xiaoge_inference" => {
                let stream = match value["stream"].as_str() {
                    Some(v @ ("road" | "wide")) => v,
                    _ => return Err("xiaoge_inference requires stream=road/wide".into()),
                };
                let frame = value["frame_id"]
                    .as_u64()
                    .and_then(|v| u32::try_from(v).ok())
                    .ok_or("xiaoge_inference requires u32 frame_id")?;
                group.group.sampled_inference_records +=
                    u64::from(value["sampled"].as_bool() == Some(true));
                for field in [
                    "latency_ms",
                    "thread_cpu_ms",
                    "queue_wait_ms",
                    "primary_ms",
                    "primary_thread_cpu_ms",
                    "compare_wall_ms",
                    "compare_thread_cpu_ms",
                ] {
                    group.json_metric(&format!("xiaoge_{field}"), &value[field]);
                    group.json_metric(&format!("xiaoge_{stream}_{field}"), &value[field]);
                }
                if let Some(comparison) = value.get("comparison").filter(|v| v.is_object()) {
                    for field in ["max_abs", "mean_abs", "max_rel"] {
                        group.json_metric(&format!("comparison_{field}"), &comparison[field]);
                    }
                    let stats = &mut group.group.comparison;
                    stats.records += 1;
                    let max_abs = comparison["max_abs"]
                        .as_f64()
                        .filter(|v| v.is_finite() && *v >= 0.0);
                    let nonfinite = comparison["nonfinite"].as_u64();
                    let bits = comparison["unequal_bits"].as_u64();
                    let shape = comparison["shape_equal"].as_bool();
                    stats.tensor_checked += u64::from(
                        max_abs.is_some()
                            || bits.is_some()
                            || shape.is_some()
                            || nonfinite.is_some(),
                    );
                    stats.tensor_unequal_records += u64::from(
                        max_abs.is_some_and(|v| v > 0.0)
                            || bits.is_some_and(|v| v > 0)
                            || nonfinite.is_some_and(|v| v > 0)
                            || shape == Some(false),
                    );
                    stats.shape_mismatches += u64::from(shape == Some(false));
                    stats.shape_checked += u64::from(shape.is_some());
                    stats.bits_checked += u64::from(bits.is_some());
                    stats.nonfinite_checked += u64::from(nonfinite.is_some());
                    stats.unequal_bits += bits.unwrap_or(0);
                    stats.nonfinite_elements += nonfinite.unwrap_or(0);
                    if let Some(equal) = comparison["decoded_equal"].as_bool() {
                        stats.decoded_checked += 1;
                        stats.decoded_mismatches += u64::from(!equal);
                    }
                    if let Some(equal) = comparison["decision_equal"].as_bool() {
                        stats.decisions_compared += 1;
                        stats.decision_mismatches += u64::from(!equal);
                    }
                    stats.errors += u64::from(
                        comparison
                            .get("error")
                            .is_some_and(|v| !v.is_null() && v.as_str() != Some("")),
                    );
                } else if recorded_key.mode == Some(2) {
                    group.group.comparison.missing_records += 1;
                }
                self.observe(
                    &key,
                    &format!("xiaoge_{stream}"),
                    mono,
                    valid,
                    FrameMetadata {
                        id: Some(frame),
                        ..FrameMetadata::default()
                    },
                );
            }
            _ => {}
        }
        Ok(())
    }
    fn event(&mut self, root: event::Reader<'_>) -> Result<()> {
        let mono = root.get_log_mono_time();
        let valid = root.get_valid();
        let key = self.timeline.key(mono);
        match root.which() {
            Ok(event::ModelV2(data)) => {
                let data = data?;
                self.observe(
                    &key,
                    "modelV2",
                    mono,
                    valid,
                    FrameMetadata {
                        id: Some(data.get_frame_id()),
                        ..FrameMetadata::default()
                    },
                );
                let group = self.group(&key);
                group.metric(
                    "driving_execution_ms",
                    f64::from(data.get_model_execution_time()) * 1000.0,
                );
                group.metric(
                    "driving_frame_drop_percent",
                    f64::from(data.get_frame_drop_perc()),
                );
            }
            Ok(event::DriverStateV2(data)) => {
                let data = data?;
                self.observe(
                    &key,
                    "driverStateV2",
                    mono,
                    valid,
                    FrameMetadata {
                        id: Some(data.get_frame_id()),
                        ..FrameMetadata::default()
                    },
                );
                let group = self.group(&key);
                group.metric(
                    "dm_execution_ms",
                    f64::from(data.get_model_execution_time()) * 1000.0,
                );
                group.metric(
                    "dm_gpu_execution_ms",
                    f64::from(data.get_gpu_execution_time()) * 1000.0,
                );
            }
            Ok(event::RoadCameraState(data))
            | Ok(event::WideRoadCameraState(data))
            | Ok(event::DriverCameraState(data)) => {
                let (name, metric) = match root.which()? {
                    event::RoadCameraState(_) => ("roadCameraState", "road_sof_interval_ms"),
                    event::WideRoadCameraState(_) => {
                        ("wideRoadCameraState", "wide_sof_interval_ms")
                    }
                    _ => ("driverCameraState", "driver_sof_interval_ms"),
                };
                let data = data?;
                self.observe(
                    &key,
                    name,
                    mono,
                    valid,
                    FrameMetadata {
                        id: Some(data.get_frame_id()),
                        sof: Some(data.get_timestamp_sof()),
                        sof_metric: Some(metric),
                    },
                );
            }
            Ok(event::DeviceState(data)) => {
                let data = data?;
                self.observe(&key, "deviceState", mono, valid, FrameMetadata::default());
                let group = self.group(&key);
                // Preserve zero as an observed counter; zero does not establish GPU idleness.
                group.metric(
                    "device_gpu_usage_percent",
                    f64::from(data.get_gpu_usage_percent()),
                );
                for (index, value) in data.get_cpu_usage_percent()?.iter().enumerate() {
                    group.metric("device_cpu_usage_percent", f64::from(value));
                    group.metric(
                        &format!("device_cpu_{index}_usage_percent"),
                        f64::from(value),
                    );
                }
                for (index, value) in data.get_cpu_temp_c()?.iter().enumerate() {
                    group.metric("device_cpu_temp_c", f64::from(value));
                    group.metric(&format!("device_cpu_{index}_temp_c"), f64::from(value));
                }
                for (index, value) in data.get_gpu_temp_c()?.iter().enumerate() {
                    group.metric("device_gpu_temp_c", f64::from(value));
                    group.metric(&format!("device_gpu_{index}_temp_c"), f64::from(value));
                }
            }
            Ok(event::LogMessage(data)) => {
                if let Some(value) = parse_log(data?.to_str()?)? {
                    self.telemetry(mono, valid, &value)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn finish(mut self, inputs: Vec<InputInfo>, format: &'static str) -> Report {
        if self.groups.is_empty() {
            self.group(&Key::unknown());
        }
        Report {
            schema: 1, input_format: format, inputs,
            mode_markers: self.timeline.markers.len(), physical_device_execution: "unknown",
            groups: self.groups.into_values().map(Accumulator::finish).collect(),
            limitations: vec![
                "Observations only; missing samples are unknown, never a pass. No acceptance or safety verdict is emitted.",
                "Physical-device execution and hardware GPU execution are unknown; effective_backend is recorded software telemetry.",
                "opencl_active_kernel_fallback_unknown and allocated graph targets do not prove that every kernel executed on the GPU.",
                "Compare mode includes shadow execution and is diagnostic only, not normal GPU performance. Pipeline/caller CPU includes shadow; compare wall/CPU is separate.",
                "Requested modes use resource mode_since_ns when present (integer u64 <= source mono_ns); legacy markers use Event.logMonoTime. Before the first accepted mode marker is unknown.",
                "Structured telemetry joins by source mono_ns; full-cereal services join by Event.logMonoTime. Legacy resource samples retain their Event clock. Shared boot identity is checked before combining inputs.",
                "Aggregate resource backends apply from snapshot time, never retroactively from mode acceptance. Mode-only/pending is unknown; mixed remains mixed. These groups are diagnostic only. Per-stream CPU fallback retains a separate CPU backend bucket within requested GPU mode.",
                "process_cpu_percent measures all process threads: one logical core is 100%, with no clamp. Per-inference thread_cpu_ms is not process CPU utilization.",
                "process_rss_mib is whole-process VmRSS (bytes / 1048576), not total GPU/device allocation.",
                "UsbGpuActive is a selection flag, not measured GPU headroom. Zero/default GPU usage is not proof of idle GPU; cereal scalars cannot distinguish absent from explicit zero.",
                "Frame ID gaps are observed discontinuities, not proof of lost inference; DM can decimate. SOF gaps count intervals over 75ms. Cross-mode intervals are excluded.",
                "Xiaoge timing summaries cover logged samples only. Sampled inference frame ID gaps are not dropped frames; sampled_inference_records identifies sampling explicitly.",
                "Tensor inequality uses exact differences/shape/bits/nonfinite; no numeric tolerance or decoded equivalence is inferred. Missing comparison fields are unobserved.",
                "decoded_equal observes exact decoded JSON including scores; decision_equal observes only the producer's lane class/geometry or BSD above_threshold comparison. Missing decision_equal is unobserved, never an agreement.",
                "Sparse BSD confidence/above_threshold comparisons with smoothing_compared=false do not validate smoothing or full temporal state, even when decision_equal=true.",
                "Quantiles use nearest rank. Device aggregate CPU/temperature distributions pool per-core/sensor readings; indexed metrics retain each source. JSONL has no cereal service or boot identity evidence.",
                "Inputs must remain unchanged during both passes. Multiple rlogs require matching nonempty bootlogId or the exact same cached InitData clock pair, plus disjoint data ranges (excluding InitData and Sentinel). Different logger sessions without boot identity must be reported separately.",
                "Without explicit boot identity a single input cannot rule out resets smaller than the one-second timestamp-regression guard.",
            ],
        }
    }
}

/// Unwrap SwagFormatter msg objects or JSON strings; ordinary text logs are ignored.
fn payload(mut value: Value) -> Result<Option<Value>> {
    for _ in 0..8 {
        if let Some(text) = value.as_str() {
            let Ok(parsed) = serde_json::from_str(text) else {
                return Ok(None);
            };
            value = parsed;
        } else if matches!(
            value["event"].as_str(),
            Some("xiaoge_inference" | "xiaoge_resources" | "xiaoge_backend")
        ) {
            return Ok(Some(value));
        } else if let Some(msg) = value.get_mut("msg") {
            value = msg.take();
        } else {
            return Ok(None);
        }
    }
    Err("structured log nesting exceeds eight levels".into())
}
fn parse_log(text: &str) -> Result<Option<Value>> {
    match serde_json::from_str(text) {
        Ok(value) => payload(value),
        Err(_) => Ok(None),
    }
}
fn scan_rlog(path: &Path, mut visitor: impl FnMut(event::Reader<'_>) -> Result<()>) -> Result<()> {
    let file = File::open(path)?;
    let input: Box<dyn Read> = if path.extension().is_some_and(|v| v == "zst") {
        Box::new(zstd::stream::read::Decoder::new(file)?)
    } else {
        Box::new(file)
    };
    let mut input = BufReader::new(input);
    let options = capnp::message::ReaderOptions {
        traversal_limit_in_words: Some(8 * 1024 * 1024),
        nesting_limit: 64,
    };
    while let Some(message) = capnp::serialize::try_read_message(&mut input, options)? {
        visitor(message.get_root::<event::Reader<'_>>()?)?;
    }
    Ok(())
}

/// Read plain or zstd-compressed concatenated full-cereal Event messages.
/// Errors (including truncation) abort the report instead of returning partial success.
pub fn report_rlogs(paths: &[PathBuf]) -> Result<Report> {
    if paths.is_empty() {
        return Err("at least one rlog is required".into());
    }
    let mut timeline = Timeline::default();
    let mut inputs = Vec::new();
    for path in paths {
        let mut info = InputInfo {
            path: path.clone(),
            ..InputInfo::default()
        };
        scan_rlog(path, |root| {
            let mono = root.get_log_mono_time();
            match root.which() {
                Ok(event::InitData(data)) => {
                    info.events += 1;
                    let data = data?;
                    info.boot(data.get_bootlog_id()?.to_str()?)?;
                    let wall = data.get_wall_time_nanos();
                    if wall != 0 {
                        let clock = [mono, wall];
                        if info.logger_init_clock.is_some_and(|old| old != clock) {
                            return Err(
                                "multiple logger InitData clocks in one input; split sessions"
                                    .into(),
                            );
                        }
                        info.logger_init_clock = Some(clock);
                    }
                }
                Ok(event::LogMessage(data)) => {
                    info.observe(mono)?;
                    if let Some(value) = parse_log(data?.to_str()?)? {
                        timeline.collect(mono, &value, root.get_valid())?;
                    }
                }
                Ok(event::Sentinel(_)) => info.events += 1,
                _ => info.observe(mono)?,
            }
            Ok(())
        })
        .map_err(|e| format!("{}: {e}", path.display()))?;
        if info.events == 0 {
            return Err(format!("{}: empty input", path.display()).into());
        }
        inputs.push(info);
    }
    inputs.sort_by_key(|i| i.first_mono_ns);
    if inputs.len() > 1 {
        let first = &inputs[0];
        for info in &inputs[1..] {
            let same_boot = match (&first.bootlog_id, &info.bootlog_id) {
                (Some(a), Some(b)) => a == b,
                (None, None) => {
                    first.logger_init_clock.is_some()
                        && first.logger_init_clock == info.logger_init_clock
                }
                _ => false,
            };
            if !same_boot {
                return Err("bootlogId/logger InitData clock differs or is missing; split reboot/unknown timelines".into());
            }
        }
        if inputs.iter().any(|info| info.first_mono_ns.is_none()) {
            return Err("multiple inputs require data events to establish disjoint ranges".into());
        }
        for pair in inputs.windows(2) {
            if pair[0].last_mono_ns >= pair[1].first_mono_ns {
                return Err("overlapping input ranges; cannot safely deduplicate events".into());
            }
        }
    }
    timeline.finish()?;
    let mut aggregator = Aggregator::new(timeline);
    for input in &inputs {
        scan_rlog(&input.path, |root| aggregator.event(root))
            .map_err(|e| format!("{}: {e}", input.path.display()))?;
    }
    Ok(aggregator.finish(inputs, "full-cereal"))
}
fn scan_jsonl(path: &Path, mut visitor: impl FnMut(u64, &Value) -> Result<()>) -> Result<()> {
    let mut input = BufReader::new(File::open(path)?);
    let mut line = Vec::new();
    let mut number = 0;
    loop {
        line.clear();
        let bytes = input
            .by_ref()
            .take(MAX_JSON_LINE + 1)
            .read_until(b'\n', &mut line)?;
        if bytes == 0 {
            break;
        }
        number += 1;
        if bytes as u64 > MAX_JSON_LINE {
            return Err(format!("JSONL line {number} exceeds 1 MiB").into());
        }
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let outer: Value =
            serde_json::from_slice(&line).map_err(|e| format!("JSONL line {number}: {e}"))?;
        let mono = outer.get("logMonoTime").and_then(Value::as_u64);
        if let Some(value) = payload(outer)? {
            let mono = mono
                .or_else(|| value["mono_ns"].as_u64())
                .ok_or("JSONL telemetry requires logMonoTime or mono_ns")?;
            visitor(mono, &value)?;
        }
    }
    Ok(())
}
/// JSONL accepts a bare telemetry payload or SwagFormatter record (msg Object/string).
/// Optional outer logMonoTime represents Event transport time; mono_ns remains the
/// source clock. Without an outer Event timestamp, mono_ns supplies both clocks.
pub fn report_jsonl(path: impl AsRef<Path>) -> Result<Report> {
    let path = path.as_ref();
    let mut timeline = Timeline::default();
    let mut info = InputInfo {
        path: path.to_owned(),
        ..InputInfo::default()
    };
    scan_jsonl(path, |mono, value| {
        info.observe(mono)?;
        timeline.collect(mono, value, true)
    })?;
    if info.events == 0 {
        return Err("JSONL contains no Xiaoge telemetry".into());
    }
    timeline.finish()?;
    let mut aggregator = Aggregator::new(timeline);
    scan_jsonl(path, |mono, value| aggregator.telemetry(mono, true, value))?;
    Ok(aggregator.finish(vec![info], "structured-jsonl"))
}
