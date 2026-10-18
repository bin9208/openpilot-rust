//! Expectations captured from the unchanged original StatLog over real ZMQ.
#![cfg(feature = "stats-producer-native")]
use openpilot_dev_tools::stats_producer_check::{self as checker, Options};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn original() -> Value {
    let bytes = include_bytes!("fixtures/stats-producer/original.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "9984d69f1112d59cbcecd5af38aeb9c4e95a19ca602f0f88a546e750adfa654e"
    );
    serde_json::from_slice(bytes).unwrap()
}
fn native() -> (PathBuf, PathBuf) {
    let binary = PathBuf::from(
        std::env::var_os("STATS_PRODUCER_CHECKER").expect("mandatory freshly built checker"),
    );
    let examples = PathBuf::from(
        std::env::var_os("STATS_PRODUCER_EXAMPLES")
            .expect("mandatory freshly built stats examples"),
    );
    assert!(binary.is_file());
    for name in ["stats_fork", "stats_pressure", "stats_types"] {
        assert!(examples.join(name).is_file());
    }
    (binary, examples)
}
#[test]
fn all_source_wire_expectations_are_independent() {
    let rows = original();
    assert_eq!(
        checker::expected_messages("reconnect").unwrap(),
        rows[0]["messages"]
    );
    assert_eq!(
        checker::expected_messages("drop").unwrap(),
        rows[1]["messages"]
    );
    let pressure = checker::expected_messages("pressure").unwrap();
    assert_eq!(
        pressure.as_array().unwrap().len(),
        rows[2]["accepted"].as_u64().unwrap() as usize
    );
    assert_eq!(pressure[0], "pressure:0.0|g");
    assert_eq!(pressure[999], "pressure:999.0|g");
    assert_eq!(
        checker::expected_messages("types").unwrap(),
        rows[3]["wire"]
    );
    assert!(checker::expected_messages("unknown").is_err());
}
#[test]
fn actual_fork_drop_pressure_and_cereal_typed_messages_match_source() {
    let (worker, examples) = native();
    let temp = tempfile::tempdir().unwrap();
    let options = Options {
        root: root(),
        examples,
        output: temp.path().join("result"),
    };
    let result = checker::run(&options, &worker).unwrap();
    assert_eq!(result["result"], "PASS");
    assert_eq!(result["reference_kind"], "captured unchanged original");
    let cases = result["native"].as_array().unwrap();
    assert_eq!(cases.len(), 4);
    for row in cases {
        let mode = row["mode"].as_str().unwrap();
        assert_eq!(row["messages"], checker::expected_messages(mode).unwrap());
        assert_eq!(row["exit"], 0);
    }
    let saved: Value =
        serde_json::from_slice(&fs::read(options.output.join("result.json")).unwrap()).unwrap();
    assert_eq!(saved, result);
}
#[test]
fn existing_output_is_preserved_on_rejection() {
    let (worker, examples) = native();
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("foreign");
    fs::create_dir(&output).unwrap();
    fs::write(output.join("result.json"), b"foreign receipt").unwrap();
    let options = Options {
        root: root(),
        examples,
        output,
    };
    assert!(checker::run(&options, &worker).is_err());
    assert_eq!(
        fs::read(options.output.join("result.json")).unwrap(),
        b"foreign receipt"
    );
}
#[test]
fn missing_example_cannot_publish_success() {
    let (worker, _) = native();
    let temp = tempfile::tempdir().unwrap();
    let options = Options {
        root: root(),
        examples: temp.path().join("missing"),
        output: temp.path().join("result"),
    };
    assert!(checker::run(&options, &worker).is_err());
    assert!(!options.output.join("result.json").exists());
}

#[test]
fn leader_signal_cleans_worker_socket_and_stops_acknowledged_child() {
    use std::{
        io::Write,
        os::unix::fs::PermissionsExt,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let (checker, _) = native();
    let root = tempfile::tempdir().unwrap();
    let examples = root.path().join("examples");
    fs::create_dir(&examples).unwrap();
    let script =
        b"#!/bin/sh\nprintf '%s\\n' \"$$\" \"$1\" > \"$STATS_TEST_READY\"\nexec /bin/sleep 60\n";
    for name in ["stats_fork", "stats_pressure", "stats_types"] {
        let path = examples.join(name);
        fs::File::create(&path).unwrap().write_all(script).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let ready = root.path().join("acknowledged");
    let output = root.path().join("result");
    struct ChildOwner(std::process::Child);
    impl Drop for ChildOwner {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut process = ChildOwner(
        Command::new(checker)
            .args([&examples, &output])
            .env("STATS_TEST_READY", &ready)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    let (pid, socket) = loop {
        if let Ok(text) = fs::read_to_string(&ready) {
            let lines: Vec<_> = text.lines().collect();
            if lines.len() == 2 {
                break (
                    lines[0].parse::<u32>().unwrap(),
                    PathBuf::from(lines[1].strip_prefix("ipc://").unwrap()),
                );
            }
        }
        assert!(Instant::now() < deadline, "child did not acknowledge");
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(socket.exists());
    rustix::process::kill_process(
        rustix::process::Pid::from_raw(process.0.id() as i32).unwrap(),
        rustix::process::Signal::INT,
    )
    .unwrap();
    let start = Instant::now();
    let status = loop {
        if let Some(status) = process.0.try_wait().unwrap() {
            break status;
        }
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "leader cancellation was not bounded"
        );
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(!status.success());
    assert!(!output.join("result.json").exists());
    loop {
        match fs::read_to_string(format!("/proc/{pid}/stat")) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Ok(text) if text.rsplit_once(") ").unwrap().1.starts_with('Z') => break,
            result => assert!(
                start.elapsed() < Duration::from_secs(2),
                "acknowledged child remains alive: {result:?}"
            ),
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let endpoint_root = socket.parent().unwrap();
    let leaked = endpoint_root.exists();
    // Preserve the failing observation, then clean this test's private namespace.
    if leaked {
        fs::remove_dir_all(endpoint_root).unwrap();
    }
    assert!(
        !leaked,
        "cancelled worker left its private socket directory"
    );
}

#[test]
fn changed_original_source_is_rejected_before_any_worker() {
    let (worker, examples) = native();
    let directory = tempfile::tempdir().unwrap();
    let source_root = directory.path().join("source");
    let pins: Value =
        serde_json::from_str(include_str!("fixtures/stats-producer/source-pins.json")).unwrap();
    for pin in pins.as_array().unwrap() {
        let name = pin["path"].as_str().unwrap();
        match fs::read(root().join(name)) {
            Ok(bytes) => {
                let destination = source_root.join(name);
                fs::create_dir_all(destination.parent().unwrap()).unwrap();
                fs::write(destination, bytes).unwrap();
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    && matches!(
                        name,
                        "rust/tools/check_stats_producer.py"
                            | "rust/tools/stats_source_producer.py"
                    ) => {}
            Err(error) => panic!("source capture failed: {error}"),
        }
    }
    let path = source_root.join("openpilot/system/statsd.py");
    let mut bytes = fs::read(&path).unwrap();
    bytes.extend_from_slice(b"\n# changed source\n");
    fs::write(path, bytes).unwrap();
    let options = Options {
        root: source_root,
        examples,
        output: directory.path().join("output"),
    };
    let error = checker::run(&options, &worker).unwrap_err();
    assert!(error.to_string().contains("source pin changed"));
    assert!(!options.output.exists());
}
