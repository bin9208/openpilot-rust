//! Inert original-only observations; never execute archived code or providers.
use super::super::{fixtures, hash, peer};
use super::{diagnostics, pipeline};
use serde_json::{json, Value};
use std::{fs, path::PathBuf};
const ZIP: &str = "3655dfbe1524303c7927bcdd0ede43599e75094fe787f0ec80d84a82cba3a516";
fn root() -> PathBuf {
    let root = PathBuf::from(
        std::env::var_os("LOGGERD_CONTROLLERS_ORIGINAL")
            .expect("externally authenticated artifact12432287942 required"),
    );
    assert_eq!(
        fs::read_to_string(root.join("archive.sha256"))
            .unwrap()
            .trim(),
        ZIP
    );
    root
}
fn read(path: impl AsRef<std::path::Path>) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
#[test]
#[ignore = "hosted; authenticated saved data only"]
fn sealed_original_diagnostics_bytes_and_source_authored_can() {
    let root = root();
    let counters = root.join("logger-counters");
    let expected = read(counters.join("report.json"));
    let peer = peer::Evidence::load(&counters.join("throughput-original"), true).unwrap();
    let rows = read(peer.root.join("diagnostics.json"));
    let messages = rows
        .as_array()
        .unwrap()
        .iter()
        .map(|r| json!([r["levelnum"], r["msg"]]))
        .collect::<Vec<_>>();
    let actual = diagnostics::throughput_report(&peer, &messages).unwrap();
    assert_eq!(
        actual, expected["original"],
        "expected rates/totals come only from original controller capture"
    );
    let mut cans = 0;
    for (index, (service, raw)) in peer.inputs.iter().enumerate() {
        if service == "can" {
            assert_eq!(
                *raw,
                fixtures::event("can", index as u32).unwrap(),
                "full source-authored CAN bytes at {index}"
            );
            cans += 1;
        }
    }
    assert!(cans > 9_000);
    let suppression = read(counters.join("suppression-original/diagnostics.json"));
    let rows = suppression.as_array().unwrap();
    assert!(rows
        .iter()
        .any(|r| r["levelnum"] == 30 && r["msg"] == "cloudlog: 2 messages suppressed"));
    assert_eq!(
        rows.iter()
            .filter(|r| r["msg"]
                .as_str()
                .unwrap()
                .contains("dropping frame waiting for audio"))
            .count(),
        4
    );
    if let Some(path) = std::env::var_os("LOGGERD_CONTROLLERS_REPLAY_REPORT") {
        super::save(&PathBuf::from(path).with_extension("diagnostics.json"),&json!({"archive_sha256":ZIP,"original_report_exact":true,"source_authored_can_packets":cans,"logged_bytes":actual["logged_bytes_at_report"]})).unwrap();
    }
}
#[test]
#[ignore = "hosted; authenticated saved data only; historical hidden collector disk is absent"]
fn sealed_original_pipeline_full_event_route_and_publications() {
    let root = root().join("logger-pipeline/original");
    let raw = fs::read(root.join("rlog.capnp")).unwrap();
    let compressed = zstd::stream::encode_all(std::io::Cursor::new(&raw), 1).unwrap();
    let (again, decoded) = crate::bootlog_artifact_check::decode_zstd(&compressed).unwrap();
    assert_eq!(raw, again);
    let expected = read(root.join("routed.json"));
    let mut logs = Vec::new();
    let mut errors = Vec::new();
    for event in decoded.as_array().unwrap() {
        for (service, target) in [("logMessage", &mut logs), ("errorLogMessage", &mut errors)] {
            if let Some(value) = event.get(service) {
                target.push(value.as_str().unwrap().to_owned());
            }
        }
    }
    assert_eq!(json!(logs), expected["logMessage"]);
    assert_eq!(json!(errors), expected["errorLogMessage"]);
    let published: Vec<String> =
        serde_json::from_value(read(root.join("publications.json"))).unwrap();
    pipeline::publication_route_check(
        &published,
        &[published.last().unwrap().clone()],
        &logs,
        &errors,
    )
    .unwrap();
    if let Some(path) = std::env::var_os("LOGGERD_CONTROLLERS_REPLAY_REPORT") {
        super::save(&PathBuf::from(path).with_extension("pipeline.json"),&json!({"archive_sha256":ZIP,"raw_rlog_sha256":hash(&raw),"complete_publication_route_equality":true,"hidden_collector_disk_replayed":false})).unwrap();
    }
}
