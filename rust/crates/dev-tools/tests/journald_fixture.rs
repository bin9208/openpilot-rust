//! Real pipe/signal regressions captured from the original journalctl fixture.
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    os::unix::process::ExitStatusExt,
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
fn bytes(text: &str) -> Vec<u8> {
    text.as_bytes()
        .chunks_exact(2)
        .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
        .collect()
}
fn wait_started(child: &mut Child, trace: &Path) {
    let start = Instant::now();
    while !fs::read(trace).is_ok_and(|v| v.ends_with(b"\n")) {
        assert!(
            child.try_wait().unwrap().is_none(),
            "fixture exited before ready"
        );
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn finish(child: &mut Child) -> std::process::ExitStatus {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if start.elapsed() >= Duration::from_secs(5) {
            child.kill().unwrap();
            let status = child.wait().unwrap();
            panic!("fixture deadline exceeded; killed {status}");
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn terminate(child: &Child) {
    rustix::process::kill_process(
        rustix::process::Pid::from_raw(i32::try_from(child.id()).unwrap()).unwrap(),
        rustix::process::Signal::TERM,
    )
    .unwrap();
}
#[test]
fn original_fixture_process_command_eof_signal_and_error_contracts() {
    let raw = include_bytes!("fixtures/journald-check/fixture-original.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(raw)),
        "44fd19114f808144e7e5f12238aa74cd4c9705284331128a86164b00feb5d547"
    );
    let corpus: serde_json::Value = serde_json::from_slice(raw).unwrap();
    let raw_base64 = include_bytes!("fixtures/journald-check/base64-original.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(raw_base64)),
        "fd237860834298f45b5d7c1beb5ba4d2763341cda115f939f1b3559d88ebc1b1"
    );
    let base64: serde_json::Value = serde_json::from_slice(raw_base64).unwrap();
    for case in corpus["cases"]
        .as_array()
        .unwrap()
        .iter()
        .chain(base64["cases"].as_array().unwrap())
    {
        let temp = tempfile::tempdir().unwrap();
        let trace = temp.path().join("trace.jsonl");
        let mut child = Command::new(env!("CARGO_BIN_EXE_journalctl-fixture"))
            .args(["-f", "-o", "json"])
            .env("JOURNAL_FIXTURE_TRACE", &trace)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        wait_started(&mut child, &trace);
        if case["signal_after"] == true {
            terminate(&child);
        } else {
            for chunk in case["chunks_hex"].as_array().unwrap() {
                child
                    .stdin
                    .as_mut()
                    .unwrap()
                    .write_all(&bytes(chunk.as_str().unwrap()))
                    .unwrap();
                child.stdin.as_mut().unwrap().flush().unwrap();
            }
        }
        drop(child.stdin.take());
        let status = finish(&mut child);
        let mut stdout = vec![];
        child
            .stdout
            .take()
            .unwrap()
            .read_to_end(&mut stdout)
            .unwrap();
        let mut stderr = vec![];
        child
            .stderr
            .take()
            .unwrap()
            .read_to_end(&mut stderr)
            .unwrap();
        assert_eq!(
            status.code().unwrap_or_else(|| -status.signal().unwrap()),
            case["exit"].as_i64().unwrap() as i32,
            "{}",
            case["name"]
        );
        assert_eq!(
            stdout,
            bytes(case["stdout_hex"].as_str().unwrap()),
            "{}",
            case["name"]
        );
        assert_eq!(
            !stderr.is_empty(),
            case["stderr_nonempty"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
        let rows = fs::read_to_string(trace)
            .unwrap()
            .lines()
            .map(|s| serde_json::from_str::<serde_json::Value>(s).unwrap())
            .collect::<Vec<_>>();
        for row in &rows {
            assert_eq!(row["pid"], child.id());
            assert_eq!(row["ppid"], std::process::id());
            assert!(row["monotonic_ns"].as_u64().unwrap() > 0);
        }
        assert_eq!(
            rows.iter().map(|v| v["event"].clone()).collect::<Vec<_>>(),
            case["events"].as_array().unwrap().clone(),
            "{}",
            case["name"]
        );
        assert_eq!(
            rows.iter().map(|v| v["argv"].clone()).collect::<Vec<_>>(),
            case["argv"].as_array().unwrap().clone(),
            "{}",
            case["name"]
        );
    }
}
#[test]
fn real_termination_interrupts_read_and_full_output_pipe() {
    use base64::Engine;
    for blocked_output in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let trace = temp.path().join("trace.jsonl");
        let mut child = Command::new(env!("CARGO_BIN_EXE_journalctl-fixture"))
            .args(["-f", "-o", "json"])
            .env("JOURNAL_FIXTURE_TRACE", &trace)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        wait_started(&mut child, &trace);
        if blocked_output {
            let command = serde_json::json!({"op":"write","base64":base64::engine::general_purpose::STANDARD.encode(vec![b'x';1024*1024])});
            writeln!(child.stdin.as_mut().unwrap(), "{command}").unwrap();
            child.stdin.as_mut().unwrap().flush().unwrap();
            let started = Instant::now();
            loop {
                assert!(child.try_wait().unwrap().is_none());
                let queued = rustix::io::ioctl_fionread(child.stdout.as_ref().unwrap()).unwrap();
                let wait = fs::read_to_string(format!("/proc/{}/wchan", child.id())).unwrap();
                if queued > 0 && wait.contains("poll") {
                    // No consumer drains this pipe, and a 1MiB payload cannot
                    // fit in its bounded kernel queue. Verify actual pending
                    // output plus blocked poll, rather than a sleep-only claim.
                    break;
                }
                assert!(
                    started.elapsed() < Duration::from_secs(5),
                    "no blocked output: {wait} {queued}"
                );
                std::thread::sleep(Duration::from_millis(2));
            }
        } else {
            writeln!(child.stdin.as_mut().unwrap(), "{{\"op\":\"close\"}}").unwrap();
            child.stdin.as_mut().unwrap().flush().unwrap();
            let mut output = vec![];
            child
                .stdout
                .take()
                .unwrap()
                .read_to_end(&mut output)
                .unwrap();
            assert!(output.is_empty());
        }
        terminate(&child);
        assert!(finish(&mut child).success());
        let records = fs::read_to_string(trace).unwrap();
        let last = records.lines().last().unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(last).unwrap()["event"],
            "signal-15"
        );
    }
}

#[test]
fn real_signal_during_close_trace_append_is_recorded_last() {
    const PEER: &str = "JOURNAL_FIXTURE_TRACE_APPEND_TEST_PEER";
    if std::env::var_os(PEER).is_some() {
        let trace = std::env::var_os("JOURNAL_FIXTURE_TRACE").unwrap();
        let status = openpilot_dev_tools::journald_fixture::run_with_hook(
            Path::new(&trace),
            &["-f".into(), "-o".into(), "json".into()],
            &mut |event| {
                if event == "stdout-closed" {
                    // The fixture body runs on a Rust test thread; raise on
                    // this same thread to match the original single-threaded
                    // Python callback, rather than racing another harness thread.
                    signal_hook::low_level::raise(signal_hook::consts::SIGTERM)?;
                }
                Ok(())
            },
        )
        .unwrap();
        std::process::exit(status);
    }
    let temp = tempfile::tempdir().unwrap();
    let trace = temp.path().join("trace.jsonl");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "real_signal_during_close_trace_append_is_recorded_last",
            "--nocapture",
        ])
        .env(PEER, "1")
        .env("JOURNAL_FIXTURE_TRACE", &trace)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"{\"op\":\"close\"}\n")
        .unwrap();
    drop(child.stdin.take());
    assert!(finish(&mut child).success());
    let records = fs::read_to_string(trace).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(records.lines().last().unwrap()).unwrap()
            ["event"],
        "signal-15"
    );
}
