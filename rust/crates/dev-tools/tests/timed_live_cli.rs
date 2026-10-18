//! Original argument failures must precede creation of the output directory.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{path::Path, process::Command};
#[test]
fn original_four_live_checker_cli_help_and_required_argument_boundaries() {
    let bytes = include_bytes!("fixtures/timed-tools/cli-original.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "e003536ede4cb062759c9941dccc61d22342a190c50c3cb582c7a8bcd861f077"
    );
    let mut original: Value = serde_json::from_slice(bytes).unwrap();
    let extra = include_bytes!("fixtures/timed-tools/live-cli-extra-original.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(extra)),
        "59a5846fea642b2267490e06d4e3b853eece00fe9268571d860ff9502a08a8e5"
    );
    let extra: Value = serde_json::from_slice(extra).unwrap();
    original["cases"]
        .as_array_mut()
        .unwrap()
        .extend(extra["cases"].as_array().unwrap().iter().cloned());
    let mut count = 0;
    for row in original["cases"].as_array().unwrap() {
        let binary = match row["script"].as_str().unwrap() {
            "check_timed_daemon.py" => env!("CARGO_BIN_EXE_check-timed-daemon"),
            "check_timed_date_daemon.py" => env!("CARGO_BIN_EXE_check-timed-date-daemon"),
            "check_timed_lifecycle.py" => env!("CARGO_BIN_EXE_check-timed-lifecycle"),
            "check_timed_shutdown.py" => env!("CARGO_BIN_EXE_check-timed-shutdown"),
            _ => continue,
        };
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("output");
        let args = row["arguments"]
            .as_array()
            .unwrap()
            .iter()
            .map(|arg| {
                arg.as_str()
                    .unwrap()
                    .replace("<output>", output.to_str().unwrap())
            })
            .collect::<Vec<_>>();
        let captured = Command::new(binary)
            .args(&args)
            .env(
                "OPENPILOT_RUST_ROOT",
                Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../..")),
            )
            .output()
            .unwrap();
        assert_eq!(
            captured.status.code(),
            row["exit_code"].as_i64().map(|code| code as i32),
            "{row}"
        );
        assert_eq!(
            String::from_utf8(captured.stdout).unwrap(),
            row["stdout"].as_str().unwrap(),
            "{row}"
        );
        assert_eq!(
            String::from_utf8(captured.stderr).unwrap(),
            row["stderr"].as_str().unwrap(),
            "{row}"
        );
        assert_eq!(
            output.exists(),
            row["output_exists"].as_bool().unwrap(),
            "{row}"
        );
        count += 1;
    }
    assert_eq!(count, 38);
}

#[test]
fn original_lifecycle_inherits_stdin_through_all_seven_actual_entrypoint_launches() {
    use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
    use std::{fs, time::Duration};
    let bytes = include_bytes!("fixtures/timed-tools/lifecycle-stdin-original.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "5a26c63b5f52d4592197fa5967c733eb71da994f162d2a8fadc4d92eb7583a6a"
    );
    let original: Value = serde_json::from_slice(bytes).unwrap();
    let launcher = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/timed-tools/lifecycle-stdin-peer.sh"
    ));
    assert_eq!(
        format!("{:x}", Sha256::digest(fs::read(launcher).unwrap())),
        original["provenance"]["launcher_sha256"]
    );
    // This regression intentionally requires the current-worktree daemon build.
    // CI builds it before the native tool suite; do not silently skip or use an
    // unrelated checkout's cache if that prerequisite is missing.
    let entry = std::env::var_os("TIMED_NATIVE_ENTRY")
        .expect("set TIMED_NATIVE_ENTRY to the current-source openpilot-timed ELF");
    assert_eq!(
        &fs::read(&entry).expect("read current timed ELF")[..4],
        b"\x7fELF"
    );
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("output");
    let probe = directory.path().join("stdin.txt");
    let mut spec = CommandSpec::new_os(
        vec![
            env!("CARGO_BIN_EXE_check-timed-lifecycle").into(),
            "--binary".into(),
            launcher.as_os_str().to_owned(),
            "--output".into(),
            output.as_os_str().to_owned(),
        ],
        Duration::from_secs(30),
    );
    spec.os_env = Some(std::collections::BTreeMap::from([
        ("TIMED_NATIVE_ENTRY".into(), entry),
        ("TIMED_STDIN_PROBE".into(), probe.as_os_str().to_owned()),
        (
            "OPENPILOT_RUST_ROOT".into(),
            Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.."))
                .as_os_str()
                .to_owned(),
        ),
    ]));
    spec.input = Some(b"original-inherited-token\n".to_vec());
    let captured = NativeRunner.run(&spec).unwrap();
    assert_eq!(
        captured.code,
        0,
        "{}",
        String::from_utf8_lossy(&captured.stderr)
    );
    assert_eq!(
        captured.stdout,
        original["original"]["stdout"].as_str().unwrap().as_bytes()
    );
    assert_eq!(
        captured.stderr,
        original["original"]["stderr"].as_str().unwrap().as_bytes()
    );
    let observations = fs::read_to_string(probe)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert_eq!(
        serde_json::json!(observations),
        original["original"]["stdin_observations"]
    );
    let summary: Value =
        serde_json::from_slice(&fs::read(output.join("summary.json")).unwrap()).unwrap();
    assert_eq!(summary["passed"], true);
    assert_eq!(summary["executions"], 7);
}
