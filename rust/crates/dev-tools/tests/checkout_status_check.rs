//! Family routing and immutable original source closure, before implementation.
use openpilot_dev_tools::checkout_status_check::{scenario_names, validate_source, Group};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn manifest() -> Value {
    let bytes = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/checkout-status/manifest.json"),
    )
    .unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        "13056a985e158f75002789fab64568ef6d28e3fa63e0db7a4f9b90729be59de1"
    );
    serde_json::from_slice(&bytes).unwrap()
}

#[test]
fn full_original_group_order_retains_every_process_and_helper_stage() {
    assert_eq!(
        scenario_names(Group::All),
        &[
            "metadata",
            "git-sha1",
            "git-sha256",
            "git-file",
            "git-executable",
            "packaged-status",
            "git-status",
            "native-helper",
            "inherited-stdio",
        ]
    );
    assert_eq!(scenario_names(Group::Metadata), &["metadata"]);
    assert_eq!(
        scenario_names(Group::Git),
        &["git-sha1", "git-sha256", "git-file", "git-executable"]
    );
    assert_eq!(
        scenario_names(Group::Status),
        &["packaged-status", "git-status"]
    );
    assert_eq!(
        scenario_names(Group::Helper),
        &["native-helper", "inherited-stdio"]
    );
}

#[test]
fn all_original_fixture_bytes_remain_bound_to_the_capture_manifest() {
    let captured = manifest();
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/checkout-status");
    assert_eq!(captured["source_sha256"].as_object().unwrap().len(), 8);
    for (name, expected) in captured["files"].as_object().unwrap() {
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(base.join(name)).unwrap())),
            expected.as_str().unwrap()
        );
    }
    let full: Value =
        serde_json::from_slice(&fs::read(base.join("full-original.json")).unwrap()).unwrap();
    let rows = full["source_results"].as_array().unwrap();
    assert_eq!(rows.len(), 7);
    assert_eq!(rows[0]["result"].as_array().unwrap().len(), 50);
    assert_eq!(rows[4]["result"].as_array().unwrap().len(), 17);
    assert_eq!(rows[5]["result"].as_array().unwrap().len(), 6);
    assert_eq!(rows[6]["result"].as_array().unwrap().len(), 44);
    assert_eq!(full["native_helper_names"].as_array().unwrap().len(), 11);
    assert_eq!(full["inherited_original"].as_array().unwrap().len(), 6);
}

#[test]
fn only_the_exact_seven_retired_tools_may_be_absent_and_reintroduction_cannot_drift() {
    let captured = manifest();
    let pins = captured["source_sha256"].as_object().unwrap();
    assert_eq!(validate_source(&root()).unwrap(), captured["source_sha256"]);
    let copy = tempfile::tempdir().unwrap();
    for (name, _) in pins {
        let target = copy.path().join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        // Later post-delete invocations may omit the seven retired files only.
        if let Ok(bytes) = fs::read(root().join(name)) {
            fs::write(&target, bytes).unwrap();
        }
    }
    for (name, _) in pins {
        let path = copy.path().join(name);
        let original = fs::read(&path).ok();
        fs::write(&path, b"changed pinned source\n").unwrap();
        assert!(validate_source(copy.path()).is_err(), "changed {name}");
        fs::remove_file(&path).unwrap();
        if name.starts_with("rust/tools/") {
            assert!(validate_source(copy.path()).is_ok(), "retired {name}");
        } else {
            assert!(validate_source(copy.path()).is_err(), "required {name}");
        }
        if let Some(bytes) = original {
            fs::write(path, bytes).unwrap();
        }
    }
    for (name, _) in pins {
        if name.starts_with("rust/tools/") {
            let _ = fs::remove_file(copy.path().join(name));
        }
    }
    assert_eq!(
        validate_source(copy.path()).unwrap(),
        captured["source_sha256"]
    );
}

#[test]
fn original_cli_admission_bytes_and_early_helper_output_phase_are_preserved() {
    let bytes = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/checkout-status/cli-original.json"),
    )
    .unwrap();
    let capture: Value = serde_json::from_slice(&bytes).unwrap();
    let binary = env!("CARGO_BIN_EXE_check-checkout-status");
    fn unhex(value: &str) -> Vec<u8> {
        value
            .as_bytes()
            .chunks_exact(2)
            .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
            .collect()
    }
    for case in capture["cases"].as_array().unwrap() {
        let work = tempfile::tempdir().unwrap();
        let output = work.path().join("out");
        let args = case["argv"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                v.as_str()
                    .unwrap()
                    .replace("{OUTPUT}", output.to_str().unwrap())
            })
            .collect::<Vec<_>>();
        let actual = std::process::Command::new(binary)
            .args(args)
            .env("OPENPILOT_RUST_ROOT", root())
            .output()
            .unwrap();
        assert_eq!(
            actual.status.code(),
            Some(case["code"].as_i64().unwrap() as i32),
            "{}",
            case["name"]
        );
        assert_eq!(
            actual.stdout,
            unhex(case["stdout_hex"].as_str().unwrap()),
            "{} stdout",
            case["name"]
        );
        assert_eq!(
            actual.stderr,
            unhex(case["stderr_hex"].as_str().unwrap()),
            "{} stderr",
            case["name"]
        );
        assert_eq!(
            output.exists(),
            case["output_exists"].as_bool().unwrap(),
            "{} output creation phase",
            case["name"]
        );
    }
}

#[test]
fn generated_metadata_inputs_preserve_all_original_raw_bytes_not_only_parsed_values() {
    let original: Value = serde_json::from_slice(
        &fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/checkout-status/metadata-policy-original.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let inputs = openpilot_dev_tools::checkout_status_check::metadata_inputs().unwrap();
    let expected = original["byte_rows"].as_array().unwrap();
    assert_eq!(inputs.len(), 43);
    assert_eq!(inputs.len(), expected.len());
    for (case, expected) in inputs.iter().zip(expected) {
        assert_eq!(case.name, expected["name"].as_str().unwrap());
        let actual = case
            .raw
            .iter()
            .map(|v| format!("{v:02x}"))
            .collect::<String>();
        assert_eq!(
            actual,
            expected["raw_hex"].as_str().unwrap(),
            "{} original raw admission bytes",
            case.name
        );
        assert_eq!(
            case.expected.as_deref(),
            expected["commit"].as_str(),
            "{} original policy expectation",
            case.name
        );
    }
}

#[test]
fn command_receipts_keep_the_original_environment_allowlist() {
    let work = tempfile::tempdir().unwrap();
    let output = work.path().join("output");
    let actual = std::process::Command::new(env!("CARGO_BIN_EXE_check-checkout-status"))
        .args([
            "--binary",
            "/bin/true",
            "--fixture",
            "/bin/true",
            "--launcher",
            "/bin/true",
            "--group",
            "metadata",
            "--output",
        ])
        .arg(&output)
        .env("OPENPILOT_RUST_ROOT", root())
        .env(
            "UNRELATED_CHECKOUT_PRIVATE_SENTINEL",
            "diagnostic-nonsecret-sentinel",
        )
        .env("CHECKOUT_RECEIPT_CONTROL", "allowed control")
        .output()
        .unwrap();
    // /bin/true deliberately closes the peer protocol. Receipts must still be
    // available, with the original environment object rather than a full dump.
    assert_eq!(actual.status.code(), Some(1), "{:?}", actual.stderr);
    let receipt: Value =
        serde_json::from_slice(&fs::read(output.join("metadata/rust/command.json")).unwrap())
            .unwrap();
    let environment = receipt["environment"]
        .as_object()
        .expect("original object schema");
    assert_eq!(environment["CHECKOUT_RECEIPT_CONTROL"], "allowed control");
    assert_eq!(environment["PATH"], "/usr/bin:/bin");
    assert!(!environment.contains_key("UNRELATED_CHECKOUT_PRIVATE_SENTINEL"));
    assert!(environment.keys().all(|key| key.starts_with("GIT_")
        || key.starts_with("CHECKOUT_")
        || matches!(key.as_str(), "PATH" | "MANAGER_DAEMON" | "TMPDIR")));
}

#[test]
fn raw_unix_paths_and_allowlisted_environment_survive_failure_receipts_losslessly() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let work = tempfile::tempdir().unwrap();
    let raw = work
        .path()
        .join(OsString::from_vec(b"checkout-\xff".to_vec()));
    // An immutable echoing peer consumes the handshake and returns a complete
    // non-ready reply. An immediately exiting peer races EOF against EPIPE.
    fs::copy("/bin/cat", &raw).unwrap();
    let output = work.path().join("output");
    let unused = work
        .path()
        .join(OsString::from_vec(b"unused-\xfe".to_vec()));
    let actual = std::process::Command::new(env!("CARGO_BIN_EXE_check-checkout-status"))
        .arg("--binary")
        .arg(&raw)
        .arg("--fixture")
        .arg(&raw)
        .args(["--launcher", "/bin/true", "--group", "metadata", "--output"])
        .arg(&output)
        .arg("--captured-binary")
        .arg(&unused)
        .arg("--handshake-fixture")
        .arg(&unused)
        .env("OPENPILOT_RUST_ROOT", root())
        .env(
            OsString::from_vec(b"CHECKOUT_RAW_\xfe".to_vec()),
            OsString::from_vec(b"value-\xff".to_vec()),
        )
        .output()
        .unwrap();
    assert_eq!(actual.status.code(), Some(1), "{:?}", actual.stderr);
    assert_eq!(actual.stderr, b"checkout peer did not report ready\n");
    let provenance = fs::read_to_string(output.join("provenance.json")).unwrap();
    assert!(provenance.contains("checkout-\\udcff"));
    assert!(provenance.contains("unused-\\udcfe"));
    let value = openpilot_log_format::JsonValue::parse(&provenance).unwrap();
    let candidate = value
        .get("native_executables")
        .unwrap()
        .get("candidate")
        .unwrap();
    assert_eq!(
        candidate.get("sha256").unwrap().to_utf8().unwrap(),
        format!("{:x}", Sha256::digest(fs::read("/bin/cat").unwrap()))
    );
    let receipt = fs::read_to_string(output.join("metadata/rust/command.json")).unwrap();
    // Both the caller provenance and resolved actual command retain raw bytes.
    assert!(receipt.contains("checkout-\\udcff"));
    assert!(receipt.contains("CHECKOUT_RAW_\\udcfe"));
    assert!(receipt.contains("value-\\udcff"));
    openpilot_log_format::JsonValue::parse(&receipt).unwrap();
}
