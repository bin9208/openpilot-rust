//! Synthetic full-cereal fixtures: no inference, native libraries or device required.
use openpilot_cereal::log_capnp::event;
use openpilot_xiaoge::log_report::{report_jsonl, report_rlogs, Group, Report};
use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "xiaoge-report-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, bytes).unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn event_bytes(mono: u64, valid: bool, fill: impl FnOnce(event::Builder<'_>)) -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    let mut root = message.init_root::<event::Builder>();
    root.set_log_mono_time(mono);
    root.set_valid(valid);
    fill(root);
    capnp::serialize::write_message_to_words(&message)
}
fn init(mono: u64, boot: &str) -> Vec<u8> {
    event_bytes(mono, true, |root| {
        root.init_init_data().set_bootlog_id(boot);
    })
}
fn log(mono: u64, payload: Value, string_msg: bool) -> Vec<u8> {
    let msg = if string_msg {
        json!(payload.to_string())
    } else {
        payload
    };
    event_bytes(mono, true, |mut root| {
        root.set_log_message(json!({"msg": msg}).to_string().as_str());
    })
}
fn marker(mono: u64, mode: u8, backend: &str) -> Vec<u8> {
    log(
        mono,
        json!({"event":"xiaoge_resources", "schema":1,"mono_ns":mono,"mode":mode,"effective_backend":backend,"resources":{"process_cpu_percent": 240.0}}),
        false,
    )
}
fn model(mono: u64, id: u32, valid: bool, seconds: f32) -> Vec<u8> {
    event_bytes(mono, valid, |root| {
        let mut data = root.init_model_v2();
        data.set_frame_id(id);
        data.set_model_execution_time(seconds);
        data.set_frame_drop_perc(12.5);
    })
}
fn group<'a>(report: &'a Report, mode: Option<u8>, backend: &str) -> &'a Group {
    report
        .groups
        .iter()
        .find(|g| g.mode == mode && g.effective_backend == backend)
        .unwrap()
}

// A latest-mode-only join or milliseconds conversion regression breaks this test.
#[test]
fn full_cereal_modes_invalidity_gaps_and_resource_units() {
    let fixture = Fixture::new();
    let mut bytes = init(1, "boot-a");
    bytes.extend(model(10, 1, true, 0.02));
    bytes.extend(marker(20, 0, "cpu"));
    bytes.extend(model(30, 2, true, 0.01));
    bytes.extend(model(40, 5, false, 0.03));
    bytes.extend(event_bytes(45, true, |root| {
        let mut data = root.init_driver_state_v2();
        data.set_frame_id(1);
        data.set_model_execution_time(0.04);
        data.set_gpu_execution_time(0.005);
    }));
    for (mono, id, sof) in [(50, 1, 10_000_000), (60, 3, 110_000_000)] {
        bytes.extend(event_bytes(mono, true, |root| {
            let mut data = root.init_road_camera_state();
            data.set_frame_id(id);
            data.set_timestamp_sof(sof);
        }));
    }
    bytes.extend(marker(70, 1, "opencl-fp32"));
    bytes.extend(model(80, 6, true, 0.05));
    let path = fixture.write("rlog", &bytes);
    let report = report_rlogs(&[path]).unwrap();
    assert_eq!(group(&report, None, "unknown").services["modelV2"].count, 1);
    let cpu = group(&report, Some(0), "cpu");
    assert_eq!(cpu.services["modelV2"].invalid, 1);
    assert_eq!(cpu.services["modelV2"].frame_id_missing, 2);
    let driving = &cpu.metrics["driving_execution_ms"];
    assert_eq!(driving.count, 2);
    assert!((driving.mean.unwrap() - 20.0).abs() < 0.001);
    assert!((driving.p95.unwrap() - 30.0).abs() < 0.001);
    assert!((cpu.metrics["dm_execution_ms"].max.unwrap() - 40.0).abs() < 0.001);
    assert!((cpu.metrics["dm_gpu_execution_ms"].mean.unwrap() - 5.0).abs() < 0.001);
    assert_eq!(cpu.metrics["process_cpu_percent"].mean, Some(240.0));
    assert_eq!(cpu.services["roadCameraState"].sof_gaps_over_75ms, 1);
    assert_eq!(cpu.metrics["road_sof_interval_ms"].mean, Some(100.0));
    assert_eq!(
        group(&report, Some(1), "opencl-fp32").metrics["driving_execution_ms"].count,
        1
    );
}

// Grouping compare timings as normal GPU or dropping string-valued msg breaks this.
#[test]
fn compare_diagnostics_tensor_and_decoded_evidence_are_explicit() {
    let fixture = Fixture::new();
    let mut bytes = marker(10, 2, "opencl-fp32");
    bytes.extend(log(20, json!({"event":"xiaoge_inference","schema":1,"mono_ns":19,"mode":2,"effective_backend":"opencl-fp32","stream":"wide","frame_id":9,"latency_ms":12.0,"thread_cpu_ms":3.0,"queue_wait_ms":1.0,"primary_ms":4.0,"compare_wall_ms":8.0,"compare_thread_cpu_ms":2.0,"comparison":{"max_abs":0.1,"mean_abs":0.01,"max_rel":0.2,"nonfinite":2,"shape_equal":false,"unequal_bits":7,"decoded_equal":false,"error":"shadow decode error"}}), true));
    let report = report_rlogs(&[fixture.write("rlog", &bytes)]).unwrap();
    let compare = group(&report, Some(2), "opencl-fp32");
    assert!(compare.diagnostic_only);
    assert_eq!(compare.metrics["xiaoge_thread_cpu_ms"].mean, Some(3.0));
    assert_eq!(compare.metrics["xiaoge_wide_latency_ms"].max, Some(12.0));
    assert_eq!(compare.comparison.tensor_unequal_records, 1);
    assert_eq!(compare.comparison.nonfinite_elements, 2);
    assert_eq!(compare.comparison.shape_checked, 1);
    assert_eq!(compare.comparison.shape_mismatches, 1);
    assert_eq!(compare.comparison.unequal_bits, 7);
    assert_eq!(compare.comparison.errors, 1);
    assert_eq!(
        compare.metrics["xiaoge_compare_thread_cpu_ms"].mean,
        Some(2.0)
    );
    assert_eq!(compare.comparison.decoded_mismatches, 1);
    assert_eq!(compare.comparison.decoded_checked, 1);
    assert!(report.groups.iter().all(|g| g.mode != Some(1)));
}

// Losing a marker at a segment boundary or depending on CLI order breaks this.
#[test]
fn same_boot_segments_share_markers_even_when_arguments_are_reversed() {
    let fixture = Fixture::new();
    let mut a = init(1, "same");
    a.extend(marker(10, 1, "opencl-fp32"));
    a.extend(event_bytes(20, true, |root| {
        root.init_sentinel();
    }));
    // Logger repeats cached InitData with its original timestamp at every segment.
    let mut b = init(1, "same");
    b.extend(event_bytes(20, true, |root| {
        root.init_sentinel();
    }));
    b.extend(model(30, 1, true, 0.02));
    let report = report_rlogs(&[fixture.write("b", &b), fixture.write("a", &a)]).unwrap();
    assert_eq!(
        group(&report, Some(1), "opencl-fp32").services["modelV2"].count,
        1
    );
}

// Current loggerd leaves bootlogId empty but repeats its exact InitData clock pair.
#[test]
fn cached_init_clock_supports_real_logger_segments_without_bootlog_id() {
    let fixture = Fixture::new();
    let header = event_bytes(1, true, |root| {
        root.init_init_data().set_wall_time_nanos(123_000_000_000);
    });
    let mut a = header.clone();
    a.extend(marker(10, 0, "cpu"));
    let mut b = header;
    b.extend(model(20, 1, true, 0.01));
    let report = report_rlogs(&[fixture.write("a", &a), fixture.write("b", &b)]).unwrap();
    assert_eq!(group(&report, Some(0), "cpu").services["modelV2"].count, 1);
    let mut reboot = event_bytes(1, true, |root| {
        root.init_init_data().set_wall_time_nanos(456_000_000_000);
    });
    reboot.extend(model(30, 2, true, 0.01));
    assert!(report_rlogs(&[
        report.inputs[0].path.clone(),
        fixture.write("reboot", &reboot)
    ])
    .is_err());
}

// Reboot mixing, unknown boot identity, overlap and truncated data must not yield a report.
#[test]
fn rejects_reboots_ambiguous_inputs_overlap_and_truncation() {
    let fixture = Fixture::new();
    let mut a = init(10, "a");
    a.extend(model(20, 1, true, 0.01));
    a.extend(model(30, 1, true, 0.01));
    let a = fixture.write("a", &a);
    let b = fixture.write("b", &init(40, "b"));
    assert!(report_rlogs(&[a.clone(), b]).is_err());
    let unknown = fixture.write("unknown", &model(40, 2, true, 0.01));
    assert!(report_rlogs(&[a.clone(), unknown]).is_err());
    let mut overlap = init(20, "a");
    overlap.extend(model(25, 2, true, 0.01));
    let overlap = fixture.write("overlap", &overlap);
    assert!(report_rlogs(&[a, overlap]).is_err());
    let mut reboot = init(2_000_000_000, "a");
    reboot.extend(init(1, "b"));
    assert!(report_rlogs(&[fixture.write("reboot", &reboot)]).is_err());
    let mut truncated = model(1, 1, true, 0.01);
    truncated.pop();
    assert!(report_rlogs(&[fixture.write("truncated", &truncated)]).is_err());
    let mut trailing = model(1, 1, true, 0.01);
    trailing.push(0);
    assert!(report_rlogs(&[fixture.write("trailing", &trailing)]).is_err());
    let mut oversized = 0_u32.to_le_bytes().to_vec();
    oversized.extend((8 * 1024 * 1024 + 1_u32).to_le_bytes());
    assert!(report_rlogs(&[fixture.write("oversized", &oversized)]).is_err());
}

// Treating an absent service or absent comparison result as a zero/pass breaks this.
#[test]
fn missing_markers_and_services_stay_unknown_and_unobserved() {
    let fixture = Fixture::new();
    let bytes = model(10, 1, false, 0.02);
    let report = report_rlogs(&[fixture.write("rlog", &bytes)]).unwrap();
    let unknown = group(&report, None, "unknown");
    assert_eq!(unknown.services["modelV2"].invalid, 1);
    assert_eq!(unknown.services["driverStateV2"].count, 0);
    assert_eq!(unknown.metrics["dm_execution_ms"].mean, None);
    assert_eq!(unknown.metrics["process_cpu_percent"].count, 0);
    assert_eq!(unknown.comparison.decoded_checked, 0);
    assert_eq!(report.mode_markers, 0);
    assert_eq!(report.physical_device_execution, "unknown");
}

// Omitting .zst decoding or swallowing an incomplete compressed stream breaks this.
#[test]
fn zstd_matches_plain_full_cereal_and_rejects_truncated_stream() {
    let fixture = Fixture::new();
    let mut bytes = marker(1, 0, "cpu");
    bytes.extend(model(2, 1, true, 0.02));
    let plain = report_rlogs(&[fixture.write("plain", &bytes)]).unwrap();
    let compressed = zstd::stream::encode_all(bytes.as_slice(), 1).unwrap();
    let packed = report_rlogs(&[fixture.write("rlog.zst", &compressed)]).unwrap();
    assert_eq!(
        serde_json::to_value(plain.groups).unwrap(),
        serde_json::to_value(packed.groups).unwrap()
    );
    assert!(
        report_rlogs(&[fixture.write("broken.zst", &compressed[..compressed.len() - 1])]).is_err()
    );
}

// Legacy markers retain conservative transport-time anchoring; nested msg forms work.
#[test]
fn jsonl_handles_nested_messages_and_keeps_missing_fields_unobserved() {
    let fixture = Fixture::new();
    let rows = [
        json!({"logMonoTime":100,"msg":{"event":"xiaoge_resources","schema":1,"mono_ns":100,"mode":0,"effective_backend":"cpu"}}),
        json!({"logMonoTime":101,"msg":json!({"event":"xiaoge_inference","schema":1,"mono_ns":101,"mode":0,"effective_backend":"cpu","stream":"road","frame_id":1,"latency_ms":5.0}).to_string()}),
    ];
    let text = rows
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    let report = report_jsonl(fixture.write("input.jsonl", text.as_bytes())).unwrap();
    let cpu = group(&report, Some(0), "cpu");
    assert_eq!(cpu.metrics["xiaoge_latency_ms"].mean, Some(5.0));
    assert_eq!(cpu.metrics["xiaoge_thread_cpu_ms"].mean, None);
    assert_eq!(cpu.services["modelV2"].count, 0);
    assert!(report_jsonl(fixture.write("bad.jsonl", b"{\"msg\":")).is_err());
}

// Backend status and invalid resource snapshots cannot manufacture a mode marker.
#[test]
fn marker_validity_conflicts_and_unknown_compare_cannot_look_like_normal_gpu() {
    let fixture = Fixture::new();
    let mut bytes = log(
        1,
        json!({"event":"xiaoge_backend","schema":1,"mono_ns":1,"mode":1,"effective_backend":"opencl-fp32"}),
        false,
    );
    bytes.extend(event_bytes(2, false, |mut root| {
        root.set_log_message(json!({"msg":{"event":"xiaoge_resources","schema":1,"mono_ns":2,"mode":1,"effective_backend":"opencl-fp32"}}).to_string().as_str());
    }));
    bytes.extend(model(3, 1, true, 0.01));
    bytes.extend(log(4, json!({"event":"xiaoge_inference","schema":1,"mono_ns":4,"mode":2,"effective_backend":"opencl-fp32","stream":"road","frame_id":1,"latency_ms":10.0}), false));
    bytes.extend(marker(5, 0, "cpu"));
    bytes.extend(log(6, json!({"event":"xiaoge_inference","schema":1,"mono_ns":6,"mode":1,"effective_backend":"opencl-fp32","stream":"road","frame_id":2,"latency_ms":6.0}), false));
    let report = report_rlogs(&[fixture.write("rlog", &bytes)]).unwrap();
    assert_eq!(report.mode_markers, 1);
    let unknown = group(&report, None, "unknown");
    assert!(unknown.diagnostic_only);
    assert_eq!(unknown.services["modelV2"].count, 1);
    assert_eq!(unknown.services["xiaoge_resources"].invalid, 1);
    assert_eq!(unknown.comparison.missing_records, 1);
    assert_eq!(unknown.mode_conflicts, 1);
    assert_eq!(
        group(&report, Some(0), "cpu").metrics["xiaoge_latency_ms"].count,
        0
    );
}

// Missing/invalid timings must not turn into zero-duration samples; quantiles are nearest rank.
#[test]
fn exact_quantiles_and_rejected_values_preserve_observation_counts() {
    let fixture = Fixture::new();
    let mut rows = vec![json!({"event":"xiaoge_resources","schema":1,"mono_ns":1,"mode":1,"effective_backend":"opencl-fp32"}).to_string()];
    for n in 1..=100 {
        rows.push(json!({"event":"xiaoge_inference","schema":1,"mono_ns":n+1,"mode":1,"effective_backend":"opencl-fp32","stream":"road","frame_id":n,"latency_ms":n}).to_string());
    }
    rows.push(json!({"event":"xiaoge_inference","schema":1,"mono_ns":102,"mode":1,"effective_backend":"opencl-fp32","stream":"road","frame_id":101,"latency_ms":-1,"thread_cpu_ms":"bad"}).to_string());
    let report =
        report_jsonl(fixture.write("quantiles.jsonl", rows.join("\n").as_bytes())).unwrap();
    let gpu = group(&report, Some(1), "opencl-fp32");
    let metric = &gpu.metrics["xiaoge_latency_ms"];
    assert_eq!(metric.count, 100);
    assert_eq!(metric.rejected, 1);
    assert!((metric.mean.unwrap() - 50.5).abs() < 1e-10);
    assert_eq!(
        (metric.p50, metric.p95, metric.p99, metric.max),
        (Some(50.0), Some(95.0), Some(99.0), Some(100.0))
    );
    assert_eq!(gpu.metrics["xiaoge_thread_cpu_ms"].rejected, 1);
    assert_eq!(gpu.metrics["xiaoge_thread_cpu_ms"].mean, None);
}

// GPU=0 is retained as telemetry, with no inference that the GPU is idle.
#[test]
fn full_cereal_wide_driver_sof_and_indexed_device_counters() {
    let fixture = Fixture::new();
    let mut bytes = marker(1, 1, "opencl-fp32");
    for (mono, id, sof) in [(2, 10, 50_000_000), (3, 11, 100_000_000)] {
        bytes.extend(event_bytes(mono, true, |root| {
            let mut data = root.init_wide_road_camera_state();
            data.set_frame_id(id);
            data.set_timestamp_sof(sof);
        }));
        bytes.extend(event_bytes(mono, false, |root| {
            let mut data = root.init_driver_camera_state();
            data.set_frame_id(id);
            data.set_timestamp_sof(sof);
        }));
    }
    bytes.extend(event_bytes(4, true, |root| {
        let mut data = root.init_device_state();
        data.set_gpu_usage_percent(0);
        data.reborrow().init_cpu_usage_percent(2).set(0, 80);
        data.reborrow().get_cpu_usage_percent().unwrap().set(1, 20);
        data.reborrow().init_cpu_temp_c(1).set(0, 60.0);
        data.init_gpu_temp_c(1).set(0, 70.0);
    }));
    bytes.extend(model(5, 1, true, f32::NAN));
    let report = report_rlogs(&[fixture.write("rlog", &bytes)]).unwrap();
    let gpu = group(&report, Some(1), "opencl-fp32");
    assert_eq!(gpu.metrics["wide_sof_interval_ms"].mean, Some(50.0));
    assert_eq!(gpu.metrics["driver_sof_interval_ms"].mean, Some(50.0));
    assert_eq!(gpu.services["driverCameraState"].invalid, 2);
    assert_eq!(gpu.metrics["device_gpu_usage_percent"].mean, Some(0.0));
    assert_eq!(gpu.metrics["device_cpu_0_usage_percent"].max, Some(80.0));
    assert_eq!(gpu.metrics["device_cpu_1_usage_percent"].max, Some(20.0));
    assert_eq!(gpu.metrics["device_cpu_temp_c"].mean, Some(60.0));
    assert_eq!(gpu.metrics["device_gpu_temp_c"].mean, Some(70.0));
    assert_eq!(gpu.metrics["driving_execution_ms"].rejected, 1);
    assert_eq!(gpu.metrics["driving_execution_ms"].mean, None);
    assert_eq!(report.physical_device_execution, "unknown");
}

// A changed schema/ambiguous mode must abort rather than silently claim comparable data.
#[test]
fn rejects_unknown_schema_mode_and_conflicting_markers() {
    let fixture = Fixture::new();
    for (name, payload) in [
        (
            "schema",
            json!({"event":"xiaoge_resources","schema":2,"mono_ns":1,"mode":0,"effective_backend":"cpu"}),
        ),
        (
            "mode",
            json!({"event":"xiaoge_resources","schema":1,"mono_ns":1,"mode":3,"effective_backend":"cpu"}),
        ),
    ] {
        assert!(report_rlogs(&[fixture.write(name, &log(1, payload, false))]).is_err());
    }
    let mut bytes = marker(1, 0, "cpu");
    bytes.extend(marker(1, 1, "opencl-fp32"));
    assert!(report_rlogs(&[fixture.write("conflict", &bytes)]).is_err());
}

// Returning to CPU after a GPU interval must not attribute that interval's gap to CPU.
#[test]
fn intervening_mode_changes_exclude_cross_mode_frame_and_sof_intervals() {
    let fixture = Fixture::new();
    let mut bytes = marker(1, 0, "cpu");
    bytes.extend(model(2, 1, true, 0.01));
    bytes.extend(event_bytes(3, true, |root| {
        let mut camera = root.init_road_camera_state();
        camera.set_frame_id(1);
        camera.set_timestamp_sof(1);
    }));
    bytes.extend(marker(4, 1, "opencl-fp32"));
    bytes.extend(marker(5, 0, "cpu"));
    bytes.extend(model(6, 10, true, 0.01));
    bytes.extend(event_bytes(7, true, |root| {
        let mut camera = root.init_road_camera_state();
        camera.set_frame_id(10);
        camera.set_timestamp_sof(1_000_000_000);
    }));
    let report = report_rlogs(&[fixture.write("rlog", &bytes)]).unwrap();
    let cpu = group(&report, Some(0), "cpu");
    assert_eq!(cpu.services["modelV2"].frame_id_missing, 0);
    assert_eq!(cpu.services["roadCameraState"].sof_gaps_over_75ms, 0);
    assert_eq!(cpu.metrics["road_sof_interval_ms"].count, 0);
}

// Resource samples do not identify either stream's effective backend; CPU fallback stays CPU.
#[test]
fn mode_only_resources_keep_backend_unknown_and_preserve_process_scope() {
    let fixture = Fixture::new();
    let mut bytes = log(
        1,
        json!({"event":"xiaoge_resources","schema":1,"mono_ns":1,"mode":1,"process_cpu_percent":250.0,"usb_gpu_active":true,"internal_gpu_busy_percent":0.0,"internal_gpu_counter_valid":true}),
        false,
    );
    bytes.extend(model(2, 1, true, 0.01));
    bytes.extend(log(3, json!({"event":"xiaoge_inference","schema":1,"mono_ns":3,"mode":1,"effective_backend":"cpu","stream":"road","frame_id":10,"sampled":true,"latency_ms":5.0,"primary_thread_cpu_ms":3.0}), false));
    let report = report_rlogs(&[fixture.write("rlog", &bytes)]).unwrap();
    let shared = group(&report, Some(1), "unknown");
    assert_eq!(shared.services["modelV2"].count, 1);
    assert_eq!(shared.metrics["process_cpu_percent"].mean, Some(250.0));
    assert_eq!(shared.usb_gpu_active_true, 1);
    assert_eq!(shared.metrics["internal_gpu_busy_percent"].mean, Some(0.0));
    let fallback = group(&report, Some(1), "cpu");
    assert_eq!(fallback.metrics["xiaoge_latency_ms"].mean, Some(5.0));
    assert_eq!(
        fallback.metrics["xiaoge_primary_thread_cpu_ms"].mean,
        Some(3.0)
    );
    assert_eq!(fallback.sampled_inference_records, 1);
    assert_eq!(fallback.metrics["process_cpu_percent"].count, 0);
}

// Shared accepts a mode before its 1 Hz resource log arrives. Aggregate backend snapshots
// must not be backdated to mode acceptance or override actual per-stream CPU fallback.
#[test]
fn authoritative_mode_since_uses_source_clock_and_keeps_aggregate_backend_separate() {
    let fixture = Fixture::new();
    let mut bytes = init(1, "boot");
    bytes.extend(model(110, 1, true, 0.01));
    bytes.extend(log(150, json!({"event":"xiaoge_resources","schema":1,"mono_ns":140,"mode_since_ns":100,"mode":1,"effective_backend":"pending"}), false));
    bytes.extend(log(160, json!({"event":"xiaoge_inference","schema":1,"mono_ns":120,"mode":1,"effective_backend":"cpu","stream":"road","frame_id":1,"latency_ms":4.0}), false));
    bytes.extend(log(170, json!({"event":"xiaoge_resources","schema":1,"mono_ns":160,"mode_since_ns":100,"mode":1,"effective_backend":"mixed"}), false));
    bytes.extend(model(180, 2, true, 0.02));
    bytes.extend(log(190, json!({"event":"xiaoge_inference","schema":1,"mono_ns":175,"mode":1,"effective_backend":"opencl-fp32","stream":"wide","frame_id":1,"latency_ms":5.0}), false));
    let report = report_rlogs(&[fixture.write("rlog", &bytes)]).unwrap();
    assert_eq!(report.mode_markers, 1);
    assert_eq!(
        group(&report, Some(1), "unknown").services["modelV2"].count,
        1
    );
    let mixed = group(&report, Some(1), "mixed");
    assert!(mixed.diagnostic_only);
    assert_eq!(mixed.services["modelV2"].count, 1);
    assert_eq!(
        group(&report, Some(1), "cpu").metrics["xiaoge_latency_ms"].mean,
        Some(4.0)
    );
    assert_eq!(
        group(&report, Some(1), "opencl-fp32").metrics["xiaoge_latency_ms"].mean,
        Some(5.0)
    );
    assert_eq!(group(&report, Some(1), "cpu").mode_conflicts, 0);
}

// Out-of-clock, negative and non-integer acceptance timestamps cannot define a boot timeline.
#[test]
fn rejects_invalid_mode_since_timestamps() {
    let fixture = Fixture::new();
    for (index, since) in [json!(11), json!(-1), json!(0.5), Value::Null]
        .into_iter()
        .enumerate()
    {
        let value = json!({"event":"xiaoge_resources","schema":1,"mono_ns":10,"mode_since_ns":since,"mode":1,"effective_backend":"pending"});
        assert!(report_rlogs(
            &[fixture.write(&format!("invalid-{index}"), &log(20, value, false))]
        )
        .is_err());
    }
}
