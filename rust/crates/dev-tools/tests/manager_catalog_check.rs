//! Independent original request-generation and source-closure contract.
use openpilot_dev_tools::manager_catalog_check::{suite_requests, validate_source};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/manager-catalog")
        .join(name)
}
fn manifest() -> Value {
    let raw = fs::read(fixture("manifest.json")).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&raw)),
        "6baafae469725142680ed1aeda918e114b1df3453d552adcd239c144f2c157a4"
    );
    serde_json::from_slice(&raw).unwrap()
}
#[test]
fn independent_original_capture_bytes_and_complete_counts_are_immutable() {
    let data = manifest();
    assert_eq!(data["source_sha256"].as_object().unwrap().len(), 24);
    assert_eq!(data["counts"]["predicate_cases"], 10573);
    assert_eq!(data["counts"]["fatal_cases"], 24);
    for (name, sha) in data["files"].as_object().unwrap() {
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(fixture(name)).unwrap())),
            sha.as_str().unwrap(),
            "{name}"
        );
    }
}
#[test]
fn every_original_request_byte_hash_order_environment_and_fatal_phase_is_generated() {
    let original: Value =
        serde_json::from_slice(&fs::read(fixture("full-original.json")).unwrap()).unwrap();
    let rows = original.as_array().unwrap();
    let generated = suite_requests().unwrap();
    assert_eq!(generated.len(), 116);
    assert_eq!(generated.len(), rows.len());
    for (actual, old) in generated.iter().zip(rows) {
        assert_eq!(actual.name, old["name"].as_str().unwrap());
        assert_eq!(actual.fatal, old["fatal"].as_bool().unwrap());
        assert_eq!(
            format!("{:x}", Sha256::digest(&actual.request)),
            old["request_sha256"].as_str().unwrap(),
            "{} exact original request bytes",
            actual.name
        );
        let encoded: serde_json::Map<String, Value> = actual
            .environment
            .iter()
            .map(|(key, value)| {
                (
                    key.clone(),
                    value
                        .as_ref()
                        .map(|value| {
                            Value::String(
                                value
                                    .as_encoded_bytes()
                                    .iter()
                                    .map(|b| format!("{b:02x}"))
                                    .collect(),
                            )
                        })
                        .unwrap_or(Value::Null),
                )
            })
            .collect();
        assert_eq!(
            Value::Object(encoded),
            old["environment_hex"],
            "{} environment snapshot",
            actual.name
        );
    }
}
#[test]
fn only_two_retired_tools_may_disappear_and_all_other_original_sources_remain_required() {
    let data = manifest();
    let pins = data["source_sha256"].as_object().unwrap();
    assert_eq!(validate_source(&root()).unwrap(), data["source_sha256"]);
    let copy = tempfile::tempdir().unwrap();
    for (name, _) in pins {
        let target = copy.path().join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        if let Ok(bytes) = fs::read(root().join(name)) {
            fs::write(target, bytes).unwrap();
        }
    }
    for (name, _) in pins {
        let path = copy.path().join(name);
        let before = fs::read(&path).ok();
        fs::write(&path, b"changed original pin\n").unwrap();
        assert!(validate_source(copy.path()).is_err(), "changed {name}");
        fs::remove_file(&path).unwrap();
        let retired = data["retired_sources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == name);
        assert_eq!(
            validate_source(copy.path()).is_ok(),
            retired,
            "missing {name}"
        );
        if let Some(bytes) = before {
            fs::write(path, bytes).unwrap();
        }
    }
    for name in data["retired_sources"].as_array().unwrap() {
        let _ = fs::remove_file(copy.path().join(name.as_str().unwrap()));
    }
    assert_eq!(validate_source(copy.path()).unwrap(), data["source_sha256"]);
}
#[test]
fn first_real_child_failure_retains_logs_and_stops_without_candidate_or_success_receipt() {
    use openpilot_dev_tools::{
        manager_catalog_check::{run, Options},
        qa_process::NativeRunner,
    };
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("failed-child");
    let result = run(
        &Options {
            root: root(),
            binary: "/bin/true".into(),
            reference: "/bin/false".into(),
            output: output.clone(),
            runner: vec![],
        },
        &mut NativeRunner,
    );
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("catalog-00 source exit 1"));
    assert!(output.join("inputs.json").is_file());
    assert!(output.join("catalog-00/request.json").is_file());
    assert!(output.join("catalog-00/source.stdout").is_file());
    assert!(output.join("catalog-00/source.stderr").is_file());
    assert!(!output.join("catalog-00/native.stdout").exists());
    assert!(!output.join("catalog-01").exists());
    assert!(!output.join("manifest.json").exists());
}
#[test]
fn command_owner_keeps_original_environment_and_resource_timeout_boundaries() {
    use openpilot_dev_tools::{
        manager_catalog_check::{run, Options},
        qa_process::{Captured, CommandSpec, Runner},
        seal::Result,
    };
    use std::{ffi::OsString, time::Duration};
    struct Inspect {
        seen: bool,
    }
    impl Runner for Inspect {
        fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
            self.seen = true;
            assert_eq!(spec.effective_argv().len(), 3);
            assert_eq!(spec.effective_argv()[0], OsString::from("/bin/false"));
            assert_eq!(spec.timeout, Duration::from_secs(600));
            assert!(spec.disable_core_dump);
            assert!(spec.inherit_stdin);
            assert!(spec
                .env_remove
                .contains(&OsString::from("OPENPILOT_PREFIX")));
            Ok(Captured {
                code: 17,
                stdout: b"source stdout\0tail".to_vec(),
                stderr: b"owned failure\xff".to_vec(),
            })
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let mut owner = Inspect { seen: false };
    let result = run(
        &Options {
            root: root(),
            binary: "/bin/true".into(),
            reference: "/bin/false".into(),
            output: temp.path().into(),
            runner: vec!["prefix-only-for-candidate".into()],
        },
        &mut owner,
    );
    assert!(result.is_err());
    assert!(owner.seen);
    assert_eq!(
        fs::read(temp.path().join("catalog-00/source.stdout")).unwrap(),
        b"source stdout\0tail"
    );
    assert_eq!(
        fs::read(temp.path().join("catalog-00/source.stderr")).unwrap(),
        b"owned failure\xff"
    );
}
#[test]
fn missing_candidate_preserves_original_completed_source_child_failure_phase() {
    use openpilot_dev_tools::{
        manager_catalog_check::{run, Options},
        qa_process::NativeRunner,
    };
    let raw = fs::read(fixture("checker-failures-original.json")).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&raw)),
        "57308a6268ce7afb14f3438aae36fadb8dbb548b81ea806983f64adfa9734224"
    );
    let captured: Vec<Value> = serde_json::from_slice(&raw).unwrap();
    let source = captured
        .iter()
        .find(|v| v["name"] == "missing-candidate")
        .unwrap();
    assert_eq!(source["source_result"], true);
    assert_eq!(source["next_scenario"], false);
    assert_eq!(source["manifest"], false);
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("output");
    let result = run(
        &Options {
            root: root(),
            reference: env!("CARGO_BIN_EXE_manager-catalog-reference").into(),
            binary: temp.path().join("missing-candidate"),
            output: output.clone(),
            runner: vec![],
        },
        &mut NativeRunner,
    );
    assert!(result.is_err());
    assert_eq!(
        output.join("catalog-00/source/result.json").is_file(),
        source["source_result"].as_bool().unwrap(),
        "missing candidate may not preempt completed source child"
    );
    assert!(!output.join("catalog-00/native/result.json").exists());
    assert!(!output.join("catalog-01").exists());
    assert!(!output.join("manifest.json").exists());
}
