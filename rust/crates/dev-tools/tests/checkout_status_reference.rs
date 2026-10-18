//! Fresh unchanged Python policy captures precede the Rust implementation.
//! Full Git/process parity is a separate real-executable nine-suite gate.
use openpilot_dev_tools::checkout_status_reference::{parse_metadata, Status};
use serde_json::Value;
use std::{cell::Cell, fs, path::Path};

fn fixture(name: &str) -> Value {
    serde_json::from_slice(
        &fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/checkout-status")
                .join(name),
        )
        .unwrap(),
    )
    .unwrap()
}

fn unhex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

#[test]
fn original_metadata_json_admission_and_exact_hex_commit_validation() {
    let capture = fixture("metadata-policy-original.json");
    assert_eq!(capture["rows"].as_array().unwrap().len(), 48);
    let rows = capture["byte_rows"].as_array().unwrap();
    assert_eq!(rows.len(), 43);
    for case in rows {
        let input = unhex(case["raw_hex"].as_str().unwrap());
        assert_eq!(
            parse_metadata(&input).as_deref(),
            case["commit"].as_str(),
            "{}",
            case["name"]
        );
    }
    // These must remain paired: metadata commit strings do not use Git strip().
    assert!(rows.iter().any(|row| row["name"] == "trailing-newline"));
    assert!(rows.iter().any(|row| row["name"] == "last-commit-wins"));
    assert!(rows.iter().any(|row| row["name"] == "integer-limit"));
    assert!(rows.iter().any(|row| row["name"] == "invalid-utf8"));
}

#[test]
fn all_original_debounce_reads_and_raw_f64_boundary_histories() {
    let capture = fixture("state-policy-original.json");
    assert_eq!(capture["rows"].as_array().unwrap().len(), 44);
    let mut status = None;
    let reads = Cell::new(0_u64);
    let mut updates = 0;
    for event in capture["events"].as_array().unwrap() {
        let installed = event["read_commit"].as_str().map(str::to_owned);
        match event["op"].as_str().unwrap() {
            "capture" => {
                // The real reference RPC must first read the checkout; this
                // policy API deliberately receives the captured read result.
                reads.set(reads.get() + 1);
                status = Some(Status::new(installed));
            }
            "update" => {
                updates += 1;
                let now = f64::from_bits(event["now_bits"].as_u64().unwrap());
                let returned = status.as_mut().unwrap().update_with(now, || {
                    reads.set(reads.get() + 1);
                    installed
                });
                assert_eq!(Some(returned), event["returned"].as_bool());
            }
            other => panic!("unrecognized original event {other}"),
        }
        let current = status.as_ref().unwrap();
        assert_eq!(current.running_commit(), event["running_commit"].as_str());
        assert_eq!(
            Some(current.reboot_required()),
            event["reboot_required"].as_bool()
        );
        assert_eq!(reads.get(), event["reads"].as_u64().unwrap());
    }
    assert_eq!(updates, 44);
}

#[test]
fn independent_inherited_exec_preserves_native_errors_and_parent_environment() {
    use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
    use std::{ffi::OsString, os::unix::fs::PermissionsExt, time::Duration};
    let work = tempfile::tempdir().unwrap();
    fs::write(work.path().join("bad-format"), b"exit 9\n").unwrap();
    fs::set_permissions(
        work.path().join("bad-format"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    fs::write(work.path().join("denied"), b"denied").unwrap();
    for (name, argv, kind, code) in [
        ("zero", "/bin/true", "child_exit", 0),
        ("one", "/bin/false", "child_exit", 1),
        ("missing", "./absent", "spawn_error", 2),
        ("format", "./bad-format", "spawn_error", 8),
        ("denied", "./denied", "spawn_error", 13),
    ] {
        let report = work.path().join(format!("{name}.json"));
        let request = serde_json::json!({"cwd":work.path(),"argv":[argv],"report":report,"environment":[["PARAMS_COPY_PATH","child-copy-path"]]});
        let mut command = CommandSpec::new_os(
            vec![
                OsString::from(env!("CARGO_BIN_EXE_checkout-status-reference")),
                "--inherited".into(),
            ],
            Duration::from_secs(4),
        );
        command.input = Some(serde_json::to_vec(&request).unwrap());
        command.os_env = Some(
            [(
                OsString::from("PARAMS_COPY_PATH"),
                OsString::from("parent-copy-path"),
            )]
            .into(),
        );
        let actual = NativeRunner.run(&command).unwrap();
        assert_eq!(
            actual.code,
            0,
            "{name}: {}",
            String::from_utf8_lossy(&actual.stderr)
        );
        let response: Value = serde_json::from_slice(&fs::read(&report).unwrap()).unwrap();
        assert_eq!(response["kind"], kind, "{name}");
        assert_eq!(
            response[if kind == "child_exit" {
                "code"
            } else {
                "errno"
            }],
            code,
            "{name}"
        );
        assert_eq!(response["parent_environment_before"], "parent-copy-path");
        assert_eq!(response["parent_environment_after"], "parent-copy-path");
    }
}
