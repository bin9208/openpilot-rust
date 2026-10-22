//! Read-only, hosted replay of sealed original-controller observations. This
//! does not execute an archived ELF or relabel old runs as current evidence.
use super::{fixtures, hash, save, scenarios, Result, SCENARIOS};
use openpilot_cereal::log_capnp::event;
use serde_json::{json, Value};
use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
};
const ARCHIVE: &str = "3655dfbe1524303c7927bcdd0ede43599e75094fe787f0ec80d84a82cba3a516";
fn decode(bytes: &[u8]) -> Result<Value> {
    let compressed = zstd::stream::encode_all(Cursor::new(bytes), 1)?;
    Ok(crate::bootlog_artifact_check::decode_zstd(&compressed)?.1)
}
fn root() -> PathBuf {
    let path = PathBuf::from(
        std::env::var_os("LOGGERD_ORIGINAL_CAPTURE").expect("sealed original capture required"),
    );
    assert_eq!(
        fs::read_to_string(path.join("archive.sha256"))
            .unwrap()
            .trim(),
        ARCHIVE
    );
    path.join("logger-native")
}
fn fixture_index(scenario: &str, service: &str, frame: u32) -> (usize, Option<bool>) {
    match scenario {
        "video" if frame == 999 => (0, Some(false)),
        "video" => ((frame % 1000) as usize, None),
        "audio" => (frame as usize, None),
        "audio-queue" => (0, Some(true)),
        "queue-restart" if service == "roadEncodeData" && (1000..1205).contains(&frame) => {
            (((frame - 1000) % 160) as usize, None)
        }
        "queue-restart" if service == "roadEncodeData" && frame == 2002 => (1, None),
        "queue-restart" => (0, None),
        _ => panic!("unexpected encoded input in {scenario}: {service}/{frame}"),
    }
}
#[test]
#[ignore = "hosted only; requires externally authenticated original artifact12432287942"]
fn sealed_original_inputs_and_full_schema_routes() {
    let root = root();
    let fixtures = fixtures::Fixtures::load(&root.join("fixtures")).unwrap();
    let expected_results: Value =
        serde_json::from_slice(&fs::read(root.join("results.json")).unwrap()).unwrap();
    let mut report = json!({"archive_sha256":ARCHIVE,"scenarios":{}});
    for scenario in SCENARIOS {
        let case = root.join(format!("{scenario}-original"));
        let expected: Vec<Value> =
            serde_json::from_slice(&fs::read(case.join("decoded.json")).unwrap()).unwrap();
        scenarios::assert_logs(scenario, &expected).unwrap();
        let mut paths = fs::read_dir(case.join("logs"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        paths.sort();
        assert_eq!(paths.len(), expected.len(), "{scenario} segments");
        assert_eq!(expected_results[scenario]["segments"], expected.len());
        for (part, path) in paths.into_iter().enumerate() {
            for name in ["rlog", "qlog"] {
                let compressed = fs::read(path.join(format!("{name}.zst"))).unwrap();
                let (raw, decoded) =
                    crate::bootlog_artifact_check::decode_zstd(&compressed).unwrap();
                assert_eq!(raw, fs::read(path.join(format!("{name}.capnp"))).unwrap());
                // No ambient-field projection: these are the same saved bytes
                // decoded independently by the original Python full schema.
                assert_eq!(decoded, expected[part][name], "{scenario}/{part}/{name}");
            }
        }
        let index: Vec<Value> =
            serde_json::from_slice(&fs::read(case.join("inputs/index.json")).unwrap()).unwrap();
        let mut ordinary = 0;
        let mut encoded = 0;
        let mut barriers = 0;
        for row in index {
            let name = row["path"].as_str().unwrap();
            assert!(!name.contains('/') && !name.contains(".."));
            let raw = fs::read(case.join("inputs").join(name)).unwrap();
            assert_eq!(row["sha256"], hash(&raw));
            let message =
                capnp::serialize::read_message(Cursor::new(&raw), Default::default()).unwrap();
            let event = message.get_root::<event::Reader<'_>>().unwrap();
            let service = row["service"].as_str().unwrap();
            if service == "logMessage"
                && match event.which().unwrap() {
                    event::LogMessage(text) => {
                        text.unwrap().to_str().unwrap() == "logger-qa-barrier"
                    }
                    _ => panic!("wrong saved logMessage union"),
                }
            {
                assert!(!event.get_valid(), "independent source barrier validity");
                let actual = fixtures::barrier();
                let mut decoded = decode(&actual).unwrap();
                // Sole nondeterministic input field: actual BOOTTIME. Keep
                // validity, union, payload and all other fields exact.
                decoded[0]["logMonoTime"] = json!(event.get_log_mono_time());
                assert_eq!(decoded, decode(&raw).unwrap());
                barriers += 1;
            } else if service.ends_with("EncodeData") {
                let data = match event.which().unwrap() {
                    event::RoadEncodeData(v)
                    | event::WideRoadEncodeData(v)
                    | event::DriverEncodeData(v)
                    | event::QRoadEncodeData(v) => v.unwrap(),
                    _ => panic!("wrong saved encoder union"),
                };
                let idx = data.get_idx().unwrap();
                let frame = idx.get_frame_id();
                let (fixture, keyframe) = fixture_index(scenario, service, frame);
                let actual = fixtures::encoded(
                    fixtures.packet(service, fixture).unwrap(),
                    service,
                    idx.get_segment_num(),
                    frame,
                    keyframe,
                )
                .unwrap();
                // Cap'n Proto pointer allocation may differ. Every schema field
                // and every Data byte must match the original source input.
                assert_eq!(
                    decode(&actual).unwrap(),
                    decode(&raw).unwrap(),
                    "{scenario}/{name}"
                );
                encoded += 1;
            } else {
                let sequence =
                    u32::try_from((event.get_log_mono_time() - 5_000_000_000) / 50_000_000)
                        .unwrap();
                assert_eq!(
                    fixtures::event(service, sequence).unwrap(),
                    raw,
                    "{scenario}/{name}"
                );
                ordinary += 1;
            }
        }
        report["scenarios"][scenario] = json!({"segments":expected.len(),"ordinary_inputs_byte_exact":ordinary,
            "encoder_inputs_all_fields_exact":encoded,"barriers_false_validity":barriers});
    }
    let output = PathBuf::from(
        std::env::var_os("LOGGERD_REPLAY_REPORT").expect("replay report path required"),
    );
    save(Path::new(&output), &report).unwrap();
}
