//! Independent original save_bootlog captures, exercised with real files/children.
use openpilot_dev_tools::bootlog_snapshot_check::{run_case, sources, Options};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, path::Path};
fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.."))
}
#[test]
fn original_capture_and_all_live_source_pins_are_immutable() {
    let bytes = include_bytes!("fixtures/bootlog-snapshot/original.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "af18f39ad3e83c9c823a4beca7917a32c69452a172dfc465e91b6b91d50a6743"
    );
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(include_bytes!(
                "fixtures/bootlog-snapshot/ambient-original.json"
            ))
        ),
        "3e390daa4198437ac3c274585234a0a03613bbb790cafed4712fc34d01b59be2"
    );
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(include_bytes!(
                "fixtures/bootlog-snapshot/output-original.json"
            ))
        ),
        "c9a66d9e35c283955531f34fd572096882b73336a5c347fba0697d8269f0f15d"
    );
    let corpus: Value = serde_json::from_slice(bytes).unwrap();
    assert_eq!(corpus["cases"].as_array().unwrap().len(), 10);
    sources(root()).unwrap();
    let temp = tempfile::tempdir().unwrap();
    for name in corpus["provenance"]["sources"].as_object().unwrap().keys() {
        let target = temp.path().join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        match fs::copy(root().join(name), target) {
            Err(error)
                if [
                    "rust/tools/check_bootlog_snapshot.py",
                    "rust/tools/bootlog_snapshot_source.py",
                ]
                .contains(&name.as_str())
                    && error.kind() == std::io::ErrorKind::NotFound => {}
            result => {
                result.unwrap();
            }
        }
    }
    for retired in [
        "rust/tools/check_bootlog_snapshot.py",
        "rust/tools/bootlog_snapshot_source.py",
    ] {
        let path = temp.path().join(retired);
        if path.exists() {
            fs::remove_file(&path).unwrap();
        }
        sources(temp.path()).unwrap();
        fs::write(&path, b"changed retired checker").unwrap();
        assert!(sources(temp.path()).is_err());
        fs::remove_file(path).unwrap();
    }
    sources(temp.path()).unwrap();
    let live = temp.path().join("openpilot/system/manager/helpers.py");
    fs::write(live, b"changed").unwrap();
    assert!(sources(temp.path()).is_err());
}
#[test]
fn complete_native_filesystem_child_snapshot_and_detached_matrix() {
    let temp = tempfile::tempdir().unwrap();
    // The narrow original C++ Params adapter is built from current source, not the
    // candidate implementation. It is shared only within this one matrix.
    let prefix = std::env::var_os("OPENPILOT_NATIVE_SDK")
        .map(std::path::PathBuf::from)
        .map(|p| p.join("usr"));
    let built = openpilot_dev_tools::bootlog_snapshot_reference::build(
        root(),
        &temp.path().join("reference"),
        prefix.as_deref(),
    )
    .unwrap();
    let options = Options {
        binary: "/unused-candidate".into(),
        reference: env!("CARGO_BIN_EXE_bootlog-snapshot-reference").into(),
        params_reference: built,
        child: env!("CARGO_BIN_EXE_bootlog-snapshot-child").into(),
        launcher: "/unused-reference-launcher".into(),
        output: temp.path().join("comparison"),
        runner: vec![],
    };
    let corpus: Value =
        serde_json::from_slice(include_bytes!("fixtures/bootlog-snapshot/original.json")).unwrap();
    for case in corpus["cases"].as_array().unwrap() {
        let observed = run_case(
            &options,
            "source",
            case["name"].as_str().unwrap(),
            &case["config"],
        )
        .unwrap();
        assert_eq!(observed, case["result"], "{}", case["name"]);
        let invocation: Value = serde_json::from_slice(
            &fs::read(
                options
                    .output
                    .join(case["name"].as_str().unwrap())
                    .join("source/invocation.json"),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(invocation["argv"]
            .as_array()
            .unwrap()
            .iter()
            .all(Value::is_string));
    }
    // Original full checker accepted this inherited child-observation flag.
    // Re-exec isolates environment changes from other concurrent tests.
    let capture = include_bytes!("fixtures/bootlog-snapshot/ambient-original.json");
    let original: Value = serde_json::from_slice(capture).unwrap();
    let target = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let candidate = std::env::var_os("BOOTLOG_SNAPSHOT_BINARY")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| target.join("examples/bootlog_snapshot"));
    let launcher = std::env::var_os("BOOTLOG_SNAPSHOT_LAUNCHER")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| target.join("openpilot-process-child"));
    assert!(candidate.is_file()&&launcher.is_file(),"build the current bootlog_snapshot example and openpilot-process-child before this required live regression");
    let ambient = temp.path().join("ambient-comparison");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_check-bootlog-snapshot"))
        .arg(&candidate)
        .arg(&options.reference)
        .arg(&ambient)
        .arg("--launcher")
        .arg(&launcher)
        .arg("--child")
        .arg(&options.child)
        .arg("--params-reference")
        .arg(&options.params_reference)
        .env("OPENPILOT_RUST_ROOT", root())
        .env("BOOTLOG_SPECIAL_COPY", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.stdout,
        original["original_stdout"].as_str().unwrap().as_bytes()
    );
    let observed: Value =
        serde_json::from_slice(&fs::read(ambient.join("ordinary/source/result.json")).unwrap())
            .unwrap();
    assert_eq!(observed, original["ordinary_source"]);
    let receipt: Value =
        serde_json::from_slice(&fs::read(ambient.join("provenance.json")).unwrap()).unwrap();
    for role in [
        "checker",
        "reference",
        "child",
        "candidate",
        "params_reference",
        "launcher",
    ] {
        let record = receipt["executables"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["role"] == role)
            .unwrap();
        let bytes = fs::read(record["path"].as_str().unwrap()).unwrap();
        assert_eq!(record["sha256"], format!("{:x}", Sha256::digest(bytes)));
    }
}
#[test]
fn native_cli_requires_inputs_and_never_claims_success_on_missing_program() {
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_check-bootlog-snapshot"))
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert!(result.stdout.is_empty());
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_bootlog-snapshot-child"))
        .env_remove("PARAMS_COPY_PATH")
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
}

#[test]
fn independently_captured_real_pipe_eof_json_and_utf8_boundaries() {
    use std::os::unix::ffi::OsStringExt;
    let bytes = include_bytes!("fixtures/bootlog-snapshot/process-original.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "b5d3e52110403ccaaabb23bac066cc03b0c525c8d68e11302cef5d431c4aefed"
    );
    let corpus: Value = serde_json::from_slice(bytes).unwrap();
    for row in corpus["cases"].as_array().unwrap() {
        let hex = row["payload_hex"].as_str().unwrap();
        let payload = hex
            .as_bytes()
            .chunks_exact(2)
            .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
            .collect();
        let mut command = std::process::Command::new("/usr/bin/printf");
        command.arg("%s").arg(std::ffi::OsString::from_vec(payload));
        let mut process =
            openpilot_dev_tools::bootlog_snapshot_check::Process::from_command(command).unwrap();
        let result = process.line();
        if let Some(expected) = row.get("value") {
            assert_eq!(result.unwrap(), *expected, "{}", row["name"]);
        } else {
            let error = result.unwrap_err();
            match row["error"]["type"].as_str().unwrap() {
                "EOFError" => {
                    assert_eq!(error.to_string(), row["error"]["message"].as_str().unwrap())
                }
                "JSONDecodeError" => assert!(
                    error.is::<serde_json::Error>(),
                    "wrong JSON error type: {error}"
                ),
                "UnicodeDecodeError" => assert!(
                    error.is::<std::str::Utf8Error>(),
                    "wrong UTF8 error type: {error}"
                ),
                kind => panic!("uncaptured error kind {kind}"),
            }
        }
        process.wait(5).unwrap();
    }
}

#[test]
fn owned_process_drop_terminates_descendant_after_successful_leader_exit() {
    use openpilot_dev_tools::bootlog_snapshot_check::Process;
    let temp = tempfile::tempdir().unwrap();
    let marker = temp.path().join("descendant");
    let mut command = std::process::Command::new("/bin/sh");
    command.args([
        "-c",
        &format!("sleep 30 & echo $! > '{}'; exit 0", marker.display()),
    ]);
    let mut child = Process::from_command(command).unwrap();
    child.wait(2).unwrap();
    let pid: i32 = fs::read_to_string(marker).unwrap().trim().parse().unwrap();
    drop(child);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    let running = loop {
        let state = fs::read_to_string(format!("/proc/{pid}/stat"));
        let alive = state.as_ref().is_ok_and(|s| {
            s.rsplit_once(") ")
                .is_some_and(|(_, tail)| !tail.starts_with('Z'))
        });
        if !alive || std::time::Instant::now() >= deadline {
            break alive;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    };
    // Clean the deliberately orphaned fixture even on the pre-repair RED.
    if running {
        let _ = rustix::process::kill_process(
            rustix::process::Pid::from_raw(pid).unwrap(),
            rustix::process::Signal::KILL,
        );
    }
    assert!(
        !running,
        "successful leader was reaped before owned descendant cleanup"
    );
}

#[test]
fn original_existing_empty_case_directory_is_refused_without_side_effects() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("ordinary/source");
    fs::create_dir_all(&output).unwrap();
    let options = Options {
        binary: "unused".into(),
        reference: "unused".into(),
        params_reference: "unused".into(),
        child: env!("CARGO_BIN_EXE_bootlog-snapshot-child").into(),
        launcher: "unused".into(),
        output: temp.path().into(),
        runner: vec![],
    };
    let error = run_case(&options, "source", "ordinary", &serde_json::json!({})).unwrap_err();
    let expected: Value = serde_json::from_slice(include_bytes!(
        "fixtures/bootlog-snapshot/output-original.json"
    ))
    .unwrap();
    assert_eq!(
        error
            .downcast_ref::<std::io::Error>()
            .and_then(std::io::Error::raw_os_error),
        expected["existing_empty_case_output"]["errno"]
            .as_i64()
            .map(|n| n as i32)
    );
    assert!(fs::read_dir(output).unwrap().next().is_none());
}

#[test]
fn invocation_preserves_original_string_arguments_and_filesystem_surrogateescape() {
    use openpilot_log_format::JsonView;
    use std::os::unix::ffi::OsStringExt;
    let encoded = openpilot_dev_tools::bootlog_snapshot_check::invocation_json(
        &[
            "/tool".into(),
            std::ffi::OsString::from_vec(vec![b'r', 0xff]),
        ],
        None,
        "joined",
        &serde_json::json!({}),
    )
    .unwrap();
    let parsed = openpilot_log_format::JsonValue::parse(&encoded).unwrap();
    let JsonView::Array(arguments) = parsed.get("argv").unwrap().view() else {
        panic!("argv must remain an array")
    };
    assert!(arguments[0].text_eq("/tool"));
    let JsonView::Text(points) = arguments[1].view() else {
        panic!("argument must remain text")
    };
    assert_eq!(points, vec![u32::from(b'r'), 0xdcff]);
    assert!(matches!(
        parsed.get("binary_sha256").unwrap().view(),
        JsonView::Null
    ));
}
