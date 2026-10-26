//! planner-ipc-native feature의 lib test로 등록한다.
//! source425 원래 Python to_dict/receipt와 실제 raw packet의 decoder 동등성 검사다.
use super::{
    corpus::{Capture, RawFile},
    decode, outputs,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, os::unix::fs::symlink, path::Path};

#[test]
fn source425_original_cpp_packets_match_original_python_policy_fields() {
    let raw = include_bytes!("fixtures/source425/receipt.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(raw)),
        "a5ac1d44574a05c1d94488c5a0a872375f97b17e85b360b487e307fd167c1b4d"
    );
    let receipt: Value = serde_json::from_slice(raw).unwrap();
    let samples: [(&str, &str, &[u8]); 3] = [
        (
            "longitudinalPlan",
            "0005-longitudinalPlan.bin",
            include_bytes!("fixtures/source425/0005-longitudinalPlan.bin"),
        ),
        (
            "lateralPlan",
            "0003-lateralPlan.bin",
            include_bytes!("fixtures/source425/0003-lateralPlan.bin"),
        ),
        (
            "driverAssistance",
            "0004-driverAssistance.bin",
            include_bytes!("fixtures/source425/0004-driverAssistance.bin"),
        ),
    ];
    let provenance: Value =
        serde_json::from_str(include_str!("fixtures/source425/PROVENANCE.json")).unwrap();
    for (service, name, bytes) in samples {
        assert_eq!(
            format!("{:x}", Sha256::digest(bytes)),
            provenance["members"][name]["sha256"].as_str().unwrap()
        );
        let expected = receipt["records"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["service"] == service && row["valid"] == true)
            .unwrap();
        assert!(expected["path"].as_str().unwrap().ends_with(name));
        let expected: decode::Record = serde_json::from_value(expected.clone()).unwrap();
        let actual = decode::packet(bytes, service, expected.path.clone()).unwrap();
        assert_eq!(expected.timestamp, actual.timestamp);
        outputs::verify_packet(bytes, &expected).unwrap();
        let mut changed = expected.clone();
        changed.timestamp ^= 1;
        assert!(outputs::verify_packet(bytes, &changed).is_err());
        changed = expected.clone();
        changed.valid = !changed.valid;
        assert!(outputs::verify_packet(bytes, &changed).is_err());
        changed = expected.clone();
        changed.data["unit-extra-field"] = serde_json::json!(true);
        assert!(outputs::verify_packet(bytes, &changed).is_err());
        assert!(decode::differences(&[expected], &[actual]).is_empty());
    }
}

#[test]
fn missing_independent_model_input_corpus_is_rejected_without_generating_inputs() {
    let root = tempfile::tempdir().unwrap();
    assert!(Capture::open(
        root.path(),
        &root.path().join("missing-audit.json"),
        &"0".repeat(64),
    )
    .is_err());
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    let missing = RawFile {
        path: "missing.bin".into(),
        bytes: 1,
        sha256: "0".repeat(64),
    };
    assert!(outputs::OriginalFile::read(root.path(), &missing).is_err());
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

// 아래 private fixture는 loader 경계 검사용이다. Cap'n Proto packet/원본 관측/retirement 근거가 아니다.
// raw는 구별 가능한 opaque token이며 native scheduler/generator/decoder를 호출해 만들지 않는다.
fn save(root: &Path, relative: &str, bytes: &[u8]) -> Value {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    serde_json::json!({"path":relative,"bytes":bytes.len(),"sha256":format!("{:x}", Sha256::digest(bytes))})
}
fn save_json(root: &Path, relative: &str, value: &Value) -> Value {
    save(root, relative, &serde_json::to_vec(value).unwrap())
}
struct SchemaFixture {
    root: tempfile::TempDir,
    audit: Value,
    index: Value,
    receipt: Value,
    pin: String,
}
impl SchemaFixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let mut sources = serde_json::Map::new();
        for name in [
            "rust/tools/plannerd_ipc.py",
            "rust/tools/plannerd_owner_fixtures.py",
            "openpilot/common/params_keys.h",
            "openpilot/cereal/log.capnp",
            "openpilot/cereal/custom.capnp",
            "opendbc_repo/opendbc/car/car.capnp",
            "openpilot/cereal/services.py",
            "openpilot/selfdrive/modeld/constants.py",
            "openpilot/cereal/messaging/__init__.py",
        ] {
            sources.insert(name.into(), Value::String("0".repeat(64)));
        }
        let mut runs = Vec::new();
        for (mode, epoch) in [("source", 0), ("source", 1), ("rust", 0), ("rust", 1)] {
            let name = format!("{mode}-{epoch}");
            let case = root.path().join("capture").join(&name);
            let mut params = serde_json::Map::new();
            for (key, bytes) in [
                ("EnableRadarTracks", b"1".as_slice()),
                ("MyDrivingModeAuto", b"0".as_slice()),
                ("UnitMarker", name.as_bytes()),
            ] {
                params.insert(key.into(), save(&case, &format!("params/{key}"), bytes));
            }
            let pending = save(
                &case,
                "CarParams.pending.bin",
                format!("unit-pending:{name}").as_bytes(),
            );
            let initial = save(
                &case,
                "events/0000-liveTracks.bin",
                format!("unit-header:{name}:initial").as_bytes(),
            );
            let mut frames = Vec::new();
            for index in 0..40 {
                let mut packets = Vec::new();
                for (p, service) in [
                    "carControl",
                    "carState",
                    "controlsState",
                    "liveParameters",
                    "radarState",
                    "selfdriveState",
                    "carrotMan",
                    "livePose",
                    "modelV2",
                ]
                .iter()
                .enumerate()
                {
                    let raw = save(
                        &case,
                        &format!("events/{:04}-{service}.bin", 1 + index * 10 + p),
                        format!("unit-header:{name}:{index}:{service}").as_bytes(),
                    );
                    packets.push(serde_json::json!({"service":service,"raw":raw}));
                }
                let live = save(
                    &case,
                    &format!("events/{:04}-liveTracks.bin", 10 + index * 10),
                    format!("unit-header:{name}:{index}:liveTracks").as_bytes(),
                );
                frames.push(serde_json::json!({"index":index,"packets":packets,"live":live}));
            }
            let manifest = serde_json::json!({
                "schema_version":1,"kind":"independent-original-planner-ipc-inputs",
                "source_commit":"0".repeat(40),"source_sha256":sources,
                "capture_provenance":{
                    "python_executable":"/unit/python","python_executable_canonical":"/unit/python",
                    "python_executable_sha256":"0".repeat(64),"python_version":"unit-schema-only",
                    "numpy_version":"unit-schema-only","origin_run_id":"1","origin_run_attempt":"1",
                    "origin_job":"unit-schema-only","host_architecture":std::env::consts::ARCH,
                    "input_transport":"actual-original-cpp-pubmaster","mode":mode,"epoch":epoch.to_string(),
                    "openpilot_prefix":mode,"daemon_command_json":serde_json::to_string(&if mode == "source" {
                        vec!["/unit/python", "-P", "rust/tools/plannerd_daemon_source.py", "--binding", "/unit/params_binding", "--source-native", "/unit/source-native"]
                    } else {
                        vec!["/unit/binary", "--solver", "/unit/solver"]
                    }).unwrap(),"params_snapshot":"before-daemon-launch"
                },
                "params":params,"car_params":pending,"initial_live":initial,"frames":frames
            });
            let manifest = save_json(
                &root.path().join("capture"),
                &format!("{name}/manifest.json"),
                &manifest,
            );
            runs.push(serde_json::json!({"mode":mode,"epoch":epoch,"events":401,"params_files":3,"manifest":manifest}));
        }
        let mut identities = serde_json::Map::new();
        let mut recorded = serde_json::Map::new();
        for role in ["binary", "params_binding", "cpp_binding", "solver_manifest"] {
            let path = if role == "solver_manifest" {
                "/unit/solver/manifest.json".into()
            } else {
                format!("/unit/{role}")
            };
            identities.insert(
                role.into(),
                serde_json::json!({"captured_path":path,"sha256":"0".repeat(64)}),
            );
            recorded.insert(path, Value::String("0".repeat(64)));
        }
        let receipt = serde_json::json!({
            "identities":recorded,
            "runs":(0..4).map(|_| serde_json::json!({"returncode":-15,"messages":3,
                "valid_counts":{"longitudinalPlan":1,"lateralPlan":1,"driverAssistance":1},
                "triggers":{"modelV2":1,"liveTracks":1},"fast_overlays":1})).collect::<Vec<_>>(),
            "comparisons":(1..4).map(|run| serde_json::json!({"run":run,"differences":[]})).collect::<Vec<_>>()
        });
        let index = serde_json::json!({"schema_version":1,"kind":"independent-original-planner-ipc-run-index",
            "status":"COMPLETE","source_commit":"0".repeat(40),"source_sha256":sources,
            "runs":runs,"events":1604,"frames":160,"car_params_files":4,"actual_output_receipt_sha256":""});
        // 원본 ZIP이 아닌 private unit token. 감사된 ZIP의 SHA pin 비교 동작만 검사한다.
        let archive = save(
            root.path(),
            "capture.zip",
            b"unit-capture-token-not-an-original-zip",
        );
        let receipt_archive = save(
            root.path(),
            "receipt.zip",
            b"unit-receipt-token-not-an-original-zip",
        );
        let audit = serde_json::json!({"schema_version":1,"kind":"audited-original-planner-ipc-capture",
            "source_commit":"0".repeat(40),"source_sha256":sources,"origin_run_id":"1","origin_run_attempt":"1",
            "origin_job":"unit-schema-only","host_architecture":std::env::consts::ARCH,
            "artifacts":{
                "capture":{"id":1,"bytes":archive["bytes"],"sha256":archive["sha256"],"archive":"capture.zip","extracted":"capture"},
                "receipt":{"id":2,"bytes":receipt_archive["bytes"],"sha256":receipt_archive["sha256"],"archive":"receipt.zip","extracted":"functional"}
            },
            "index":{},"receipt":{},"identities":identities});
        let mut fixture = Self {
            root,
            audit,
            index,
            receipt,
            pin: String::new(),
        };
        fixture.seal();
        fixture
    }
    fn seal(&mut self) {
        self.audit["receipt"] = save_json(
            &self.root.path().join("functional"),
            "receipt.json",
            &self.receipt,
        );
        self.index["actual_output_receipt_sha256"] = self.audit["receipt"]["sha256"].clone();
        self.audit["index"] =
            save_json(&self.root.path().join("capture"), "index.json", &self.index);
        self.pin = save_json(self.root.path(), "audit.json", &self.audit)["sha256"]
            .as_str()
            .unwrap()
            .into();
    }
    fn open(&self) -> Result<Capture, Box<dyn std::error::Error>> {
        Capture::open(
            self.root.path(),
            &self.root.path().join("audit.json"),
            &self.pin,
        )
    }
}

#[test]
fn writer_schema_selects_each_mode_epoch_without_reusing_or_generating_packets() {
    let fixture = SchemaFixture::new();
    let capture = fixture.open().unwrap();
    let mut headers = std::collections::BTreeSet::new();
    for (mode, epoch) in [("source", 0), ("source", 1), ("rust", 0), ("rust", 1)] {
        let case = capture.select(mode, epoch).unwrap();
        let name = format!("{mode}-{epoch}");
        assert_eq!(
            case.read(&case.manifest.params["UnitMarker"]).unwrap(),
            name.as_bytes()
        );
        assert_eq!(
            case.read(&case.manifest.car_params).unwrap(),
            format!("unit-pending:{name}").as_bytes()
        );
        let raw = case.read(&case.manifest.initial_live).unwrap();
        assert_eq!(raw, format!("unit-header:{name}:initial").as_bytes());
        headers.insert(raw);
    }
    assert_eq!(headers.len(), 4);
    assert!(capture.select("source", 2).is_err());
    assert!(capture.select("unknown", 0).is_err());
    capture.verify().unwrap();
    let inputs: [(&str, &[u8]); 2] = [
        (
            "x86_64",
            include_bytes!("../../../tests/fixtures/planner-ipc-original-audit/x86_64.json"),
        ),
        (
            "aarch64",
            include_bytes!("../../../tests/fixtures/planner-ipc-original-audit/aarch64.json"),
        ),
    ];
    for (architecture, original_input) in inputs {
        let audit =
            outputs::OutputAudit::parse(outputs::audit_bytes(architecture).unwrap()).unwrap();
        assert_eq!(audit.runs.len(), 4);
        assert!(audit.runs.iter().all(|r| r.publications.len() == 121));
        let input: Value = serde_json::from_slice(original_input).unwrap();
        let sha = format!("{:x}", Sha256::digest(original_input));
        let artifact = input["artifacts"]["receipt"]["id"].as_u64().unwrap();
        let summary = input["receipt"]["path"].as_str().unwrap();
        audit.bind_input(&sha, artifact, summary).unwrap();
        assert!(audit
            .bind_input(&"0".repeat(64), artifact, summary)
            .is_err());
        assert!(audit.bind_input(&sha, artifact + 1, summary).is_err());
        assert!(audit
            .bind_input(&sha, artifact, "tmp/foreign/receipt.json")
            .is_err());
        // private opaque input metadata를 역사 outputs의 실제 origin으로 받아들이면 안 된다.
        assert!(audit.bind(&capture).is_err());
    }
}

#[test]
fn capture_auth_rejects_unpinned_swapped_incomplete_and_reappeared_data() {
    let mut fixture = SchemaFixture::new();
    let old_pin = fixture.pin.clone();
    fixture.audit["artifacts"]["capture"]["id"] = serde_json::json!(3);
    fixture.seal();
    assert!(Capture::open(
        fixture.root.path(),
        &fixture.root.path().join("audit.json"),
        &old_pin
    )
    .is_err());
    let original_index = fixture.index.clone();
    for (field, wrong) in [
        ("status", serde_json::json!("INCOMPLETE")),
        ("events", serde_json::json!(1603)),
    ] {
        fixture.index[field] = wrong;
        fixture.seal();
        assert!(fixture.open().is_err(), "index {field}");
        fixture.index = original_index.clone();
    }
    fixture.index["runs"][2]["manifest"] = fixture.index["runs"][0]["manifest"].clone();
    fixture.seal();
    assert!(fixture.open().is_err(), "mode/epoch alias");
    fixture.index = original_index;
    fixture.receipt["runs"][0]["fast_overlays"] = serde_json::json!(0);
    fixture.seal();
    assert!(fixture.open().is_err(), "original actual output failure");
    fixture.receipt["runs"][0]["fast_overlays"] = serde_json::json!(1);
    fixture.receipt["identities"]["/unit/cpp_binding"] = serde_json::json!("1".repeat(64));
    fixture.seal();
    assert!(fixture.open().is_err(), "binding identity substitution");
    fixture.receipt["identities"]["/unit/cpp_binding"] = serde_json::json!("0".repeat(64));
    fixture.seal();
    let capture = fixture.open().unwrap();
    let case = capture.select("rust", 1).unwrap();
    let packet = case.path(&case.manifest.initial_live).unwrap();
    let saved = fs::read(&packet).unwrap();
    fs::remove_file(&packet).unwrap();
    let replaced = fixture.root.path().join("replacement.bin");
    fs::write(&replaced, &saved).unwrap();
    symlink(&replaced, &packet).unwrap();
    assert!(capture.verify().is_err(), "same-byte symlink reappearance");
    fs::remove_file(&packet).unwrap();
    fs::write(&packet, saved).unwrap();
    capture.verify().unwrap();
    fs::write(fixture.root.path().join("capture.zip"), b"changed archive").unwrap();
    assert!(
        capture.verify().is_err(),
        "artifact changed after admission"
    );

    // 기존 원본 metadata의 경계만 변조한다. 새 expected/model packet은 만들지 않는다.
    let audit: Value = serde_json::from_slice(outputs::audit_bytes("x86_64").unwrap()).unwrap();
    for field in [
        "extra-field",
        "missing-case",
        "missing-packet",
        "alias-case",
        "receipt-cap",
        "packet-cap",
        "total-cap",
        "path-escape",
    ] {
        let mut changed = audit.clone();
        match field {
            "extra-field" => changed["extra-field"] = serde_json::json!(true),
            "missing-case" => {
                let _ = changed["runs"].as_array_mut().unwrap().pop();
            }
            "missing-packet" => {
                let _ = changed["runs"][0]["publications"]
                    .as_array_mut()
                    .unwrap()
                    .pop();
            }
            "alias-case" => changed["runs"][1]["receipt"] = changed["runs"][0]["receipt"].clone(),
            "receipt-cap" => changed["runs"][0]["receipt"]["bytes"] = serde_json::json!(1048577),
            "packet-cap" => {
                changed["runs"][0]["publications"][0]["bytes"] = serde_json::json!(1048577)
            }
            "total-cap" => {
                for raw in changed["runs"][0]["publications"].as_array_mut().unwrap() {
                    raw["bytes"] = serde_json::json!(1048576);
                }
            }
            "path-escape" => {
                changed["runs"][0]["publications"][0]["path"] = serde_json::json!("../foreign.bin")
            }
            _ => unreachable!(),
        }
        assert!(
            outputs::OutputAudit::parse(&serde_json::to_vec(&changed).unwrap()).is_err(),
            "{field}"
        );
    }
    let original = include_bytes!("fixtures/source425/0005-longitudinalPlan.bin");
    for kind in [
        "file",
        "symlink",
        "dangling-symlink",
        "directory",
        "same-size",
        "truncated",
    ] {
        let root = tempfile::tempdir().unwrap();
        let raw: RawFile =
            serde_json::from_value(save(root.path(), "original.bin", original)).unwrap();
        let (bytes, guard) = outputs::OriginalFile::read(root.path(), &raw).unwrap();
        assert_eq!(bytes, original);
        guard.verify().unwrap();
        let path = root.path().join("original.bin");
        match kind {
            "same-size" => {
                let mut changed = bytes;
                changed[0] ^= 1;
                fs::write(&path, changed).unwrap();
            }
            "truncated" => fs::write(&path, &bytes[..bytes.len() - 1]).unwrap(),
            _ => {
                let saved = root.path().join("saved.bin");
                fs::rename(&path, &saved).unwrap();
                match kind {
                    "file" => fs::write(&path, &bytes).unwrap(),
                    "symlink" => symlink(&saved, &path).unwrap(),
                    "dangling-symlink" => symlink(root.path().join("absent"), &path).unwrap(),
                    "directory" => fs::create_dir(&path).unwrap(),
                    _ => unreachable!(),
                }
            }
        }
        assert!(
            guard.verify().is_err(),
            "original output {kind} mutation/reappearance"
        );
    }

    let root = tempfile::tempdir().unwrap();
    super::require_retired_scheduler_test_absent(root.path()).unwrap();
    let error = super::finish_scheduler_retirement(root.path(), Err("original work exit42".into()))
        .unwrap_err();
    assert_eq!(error.to_string(), "original work exit42");
    assert_eq!(
        super::finish_scheduler_retirement(root.path(), Ok(serde_json::json!(42))).unwrap(),
        serde_json::json!(42)
    );
    for kind in ["file", "symlink", "dangling-symlink", "directory"] {
        let root = tempfile::tempdir().unwrap();
        let retired = root
            .path()
            .join("rust/tools/tests/test_plannerd_native_scheduler.py");
        fs::create_dir_all(retired.parent().unwrap()).unwrap();
        super::require_retired_scheduler_test_absent(root.path()).unwrap();
        match kind {
            "file" => fs::write(&retired, b"reappeared scheduler test").unwrap(),
            "directory" => fs::create_dir(&retired).unwrap(),
            "symlink" => {
                let target = root.path().join("original-source");
                fs::write(&target, b"reappeared scheduler test").unwrap();
                symlink(&target, &retired).unwrap();
            }
            "dangling-symlink" => symlink(root.path().join("absent"), &retired).unwrap(),
            _ => unreachable!(),
        }
        assert!(super::require_retired_scheduler_test_absent(root.path()).is_err());
        for work in [Ok(serde_json::json!(0)), Err("original work exit42".into())] {
            assert!(
                super::finish_scheduler_retirement(root.path(), work)
                    .unwrap_err()
                    .to_string()
                    .contains("retired scheduler test reappeared"),
                "{kind}"
            );
        }
    }
}
