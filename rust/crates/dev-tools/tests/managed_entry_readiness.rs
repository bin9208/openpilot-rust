//! Force the original subscriber-before-publisher race through real queues.
#![cfg(feature = "managed-entry-native")]
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn check_readiness(output: &Path) {
    let binary = PathBuf::from(
        std::env::var_os("MANAGED_CHILD_BINARY").expect("mandatory managed child artifact"),
    );
    let collector = PathBuf::from(
        std::env::var_os("MANAGED_COLLECTOR_BINARY").expect("mandatory collector artifact"),
    );
    let binding = PathBuf::from(
        std::env::var_os("MANAGED_PARAMS_BINDING").expect("mandatory binding provenance"),
    );
    let result = Command::new(env!("CARGO_BIN_EXE_check-managed-entry"))
        .arg("--readiness")
        .args([&binary, &collector, &binding])
        .arg(output)
        .env(
            "OPENPILOT_RUST_ROOT",
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."),
        )
        .env("PATH", "/nonexistent")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "actual readiness CLI failed, evidence {}: {}",
        output.display(),
        String::from_utf8_lossy(&result.stderr)
    );
    let report: Value =
        serde_json::from_slice(&fs::read(output.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(report["result"], "PASS");
    assert_eq!(report["scenario"], "reset-error");
    assert_eq!(report["schedule"], "subscriber-before-publisher");
    assert_eq!(report["error_messages"], 1);
    let case = output.join("reset-error");
    let order: Value = serde_json::from_slice(
        &fs::read(case.join("collector/initialization-order.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(order["subscriber_created"], true);
    assert_eq!(order["publisher_initialized_after_subscriber"], true);
    let actual: Value =
        serde_json::from_slice(&fs::read(case.join("result.json")).unwrap()).unwrap();
    assert_eq!(actual["response"]["outcome"]["kind"], "raised");
    assert_eq!(
        actual["records"]["errorLogMessage"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        actual["records"]["errorLogMessage"],
        actual["records"]["logMessage"]
    );
    let diagnostic: Value =
        serde_json::from_slice(&fs::read(case.join("collector/queue-diagnostic.json")).unwrap())
            .unwrap();
    assert_eq!(diagnostic["error_queue_contains_crash"], true);
    assert_eq!(diagnostic["error_queue_sha256"].as_str().unwrap().len(), 64);
}

#[test]
fn source_reset_error_reconnects_after_publisher_initializes_late() {
    let output = tempfile::tempdir().unwrap().keep();
    check_readiness(&output);
    fs::remove_dir_all(output).unwrap();
}

#[test]
fn original_pipe_gate_accepts_output_longer_than_unix_socket_address() {
    let temporary = tempfile::tempdir().unwrap().keep();
    let output = temporary
        .join("long-output-component-".repeat(7))
        .join("readiness");
    assert!(output.as_os_str().as_encoded_bytes().len() > 108);
    fs::create_dir_all(&output).unwrap();
    check_readiness(&output);
    fs::remove_dir_all(temporary).unwrap();
}

#[test]
fn readiness_source_preflight_failure_invalidates_previous_success() {
    let root = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    fs::write(
        output.path().join("manifest.json"),
        r#"{"result":"PASS","scenario":"reset-error"}"#,
    )
    .unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_check-managed-entry"))
        .args(["--readiness", "/bin/true", "/bin/true", "/missing-binding"])
        .arg(output.path())
        .env("OPENPILOT_RUST_ROOT", root.path())
        .env("PATH", "/nonexistent")
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(
        !output.path().join("manifest.json").exists(),
        "readiness preflight left a stale PASS"
    );
}

#[test]
fn exactly_six_retired_sources_allow_absence_with_separate_historical_evidence() {
    use std::os::unix::fs::symlink;
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let expected: Value = serde_json::from_str(include_str!(
        "fixtures/managed-entry/readiness-source-pins.json"
    ))
    .unwrap();
    let pins = expected["sha256"].as_object().unwrap();
    let retired = [
        "rust/tools/check_version_reference.py",
        "rust/tools/tombstoned_reference.py",
        "rust/tools/check_tombstoned_reference.py",
        "rust/tools/check_managed_entry.py",
        "rust/tools/check_managed_entry_readiness.py",
        "rust/tools/managed_entry_reference.py",
    ];
    let root = tempfile::tempdir().unwrap();
    let mut originals = std::collections::BTreeMap::new();
    for name in pins.keys() {
        let source = repository.join(name);
        let target = root.path().join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        match fs::symlink_metadata(&source) {
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    && retired.contains(&name.as_str()) =>
            {
                continue
            }
            _ => {}
        }
        let bytes = fs::read(source).unwrap();
        originals.insert(name.clone(), bytes.clone());
        if !retired.contains(&name.as_str()) {
            fs::write(target, bytes).unwrap();
        }
    }
    let invoke = || {
        let output = tempfile::tempdir().unwrap();
        fs::write(output.path().join("manifest.json"), r#"{"result":"PASS"}"#).unwrap();
        let result = Command::new(env!("CARGO_BIN_EXE_check-managed-entry"))
            .arg("--readiness")
            .arg(
                std::env::var_os("MANAGED_CHILD_BINARY").expect("mandatory managed child artifact"),
            )
            .arg(
                std::env::var_os("MANAGED_COLLECTOR_BINARY").expect("mandatory collector artifact"),
            )
            .arg(std::env::var_os("MANAGED_PARAMS_BINDING").expect("mandatory binding provenance"))
            .arg(output.path())
            .env("OPENPILOT_RUST_ROOT", root.path())
            .env("PATH", "/nonexistent")
            .output()
            .unwrap();
        let manifest = fs::read(output.path().join("manifest.json")).ok();
        if result.status.success() {
            Some(serde_json::from_slice::<Value>(&manifest.unwrap()).unwrap())
        } else {
            eprintln!(
                "readiness admission rejected: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert!(
                manifest.is_none(),
                "failed admission left stale success: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            None
        }
    };
    let missing =
        invoke().expect("literal absence of only the six retired tools must remain executable");
    for (name, hash) in pins {
        let (correct, other) = if retired.contains(&name.as_str()) {
            ("historical_source_sha256", "source_sha256")
        } else {
            ("source_sha256", "historical_source_sha256")
        };
        assert_eq!(&missing[correct][name], hash, "{name}");
        assert!(missing[other].get(name).is_none(), "mislabelled {name}");
    }
    assert_eq!(
        missing["historical_source_sha256"]
            .as_object()
            .unwrap()
            .len(),
        6
    );
    assert_eq!(
        missing["source_sha256"].as_object().unwrap().len(),
        pins.len() - 6
    );
    for name in retired {
        let file = root.path().join(name);
        fs::write(&file, b"changed restored source").unwrap();
        assert!(invoke().is_none(), "changed restored {name}");
        fs::remove_file(&file).unwrap();
        symlink("missing-original", &file).unwrap();
        assert!(invoke().is_none(), "dangling {name}");
        fs::remove_file(&file).unwrap();
        fs::create_dir(&file).unwrap();
        assert!(invoke().is_none(), "directory {name}");
        fs::remove_dir(&file).unwrap();
        if let Some(bytes) = originals.get(name) {
            fs::write(&file, bytes).unwrap();
            let restored = invoke().expect("correct original reintroduction must be live evidence");
            assert_eq!(restored["source_sha256"][name], pins[name]);
            assert!(restored["historical_source_sha256"].get(name).is_none());
            fs::remove_file(&file).unwrap();
        }
    }
    for (name, bytes) in originals {
        if retired.contains(&name.as_str()) {
            continue;
        }
        let file = root.path().join(&name);
        fs::remove_file(&file).unwrap();
        assert!(invoke().is_none(), "missing retained {name}");
        fs::write(&file, b"changed retained source").unwrap();
        assert!(invoke().is_none(), "changed retained {name}");
        fs::write(file, bytes).unwrap();
    }
}

#[test]
fn nonexistent_repository_invalidates_previous_success_in_both_modes() {
    for readiness in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        fs::write(output.path().join("manifest.json"), r#"{"result":"PASS"}"#).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_check-managed-entry"));
        if readiness {
            command.arg("--readiness");
        }
        let result = command
            .args(["/bin/true", "/bin/true", "/missing-binding"])
            .arg(output.path())
            .env(
                "OPENPILOT_RUST_ROOT",
                root.path().join("missing-repository"),
            )
            .env("PATH", "/nonexistent")
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(
            !output.path().join("manifest.json").exists(),
            "nonexistent root left PASS, readiness={readiness}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
}

#[test]
fn readiness_original_accepts_raw_parent_directory_bytes() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    let temporary = tempfile::tempdir().unwrap().keep();
    let output = temporary
        .join(OsString::from_vec(b"raw-\xff".to_vec()))
        .join("ordinary");
    fs::create_dir_all(&output).unwrap();
    check_readiness(&output);
    fs::remove_dir_all(temporary).unwrap();
}
