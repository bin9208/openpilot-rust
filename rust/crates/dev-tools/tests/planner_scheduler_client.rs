//! Python을 실행하지 않는다. client13 의미와 원본 parse35, 실제 생산 frame loop를 검사한다.
use openpilot_dev_tools::{planner_ci_controller::scheduler, seal};
#[path = "../src/planner_ci_controller/planner_ipc_native/frame_loop.rs"]
mod frame_loop;
#[path = "../src/planner_ci_controller/scheduler_client.rs"]
mod scheduler_client;
use scheduler_client::{parse_response, ResponseError, SchedulerClient};
use seal::Result;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

fn peer(directory: &Path, mode: &str) -> PathBuf {
    let path = directory.join("scheduler-peer");
    let reply = match mode {
        "malformed" => "printf 'invalid\\n'",
        "nonfinite" => "printf 'nan\\n'",
        "long" => "printf '%0300d\\n' 0",
        "multiple" => "printf '0.0\\n1.0\\n'",
        "observed" => "cat -- \"$root/response.line\"; printf '\\n'",
        "timeout" => "exec /bin/sleep 30",
        _ => "printf '0.0\\n'",
    };
    let before = match mode {
        "eof" => "exit 0",
        "exit42" => "exit 42",
        "ignore-term" => "trap '' TERM",
        _ => ":",
    };
    let after = match mode {
        "close42" => "exit 42",
        "ignore-term" => "exec /bin/sleep 30",
        _ => "exit 0",
    };
    fs::write(&path,format!("#!/bin/sh\ntest \"$1\" = schedule || exit 99\nroot=$(dirname -- \"$0\")\n{before}\nwhile IFS= read -r line; do\nprintf '%s\\n' \"$line\" >> \"$root/requests.tsv\"\n{reply}\ndone\n{after}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}
fn client(directory: &Path, mode: &str) -> SchedulerClient {
    SchedulerClient::canonical(Some(&peer(directory, mode)), &directory.join("stderr.log")).unwrap()
}

#[test]
fn independent_python_oracle_bytes_remain_pinned_without_execution() {
    let raw = include_str!("../../../tools/plannerd_ipc.py");
    let start = raw.find("def next_model_target(").unwrap();
    let tail = &raw[start..];
    let end = tail.find("\n  return target\n").unwrap() + "\n  return target\n".len();
    let function = &tail[..end];
    assert_eq!(
        format!("{:x}", Sha256::digest(function.as_bytes())),
        "a04e889c725715395baae751a5b3526c0c686b0075d67ab95b95fb94bd987be1"
    );
}

struct Frames {
    models: usize,
    lives: usize,
    waits: Vec<f64>,
}
impl frame_loop::FrameEvents for Frames {
    fn sleep_until(&mut self, target: f64) -> Result<()> {
        self.waits.push(target);
        Ok(())
    }
    fn model(&mut self, index: usize) -> Result<(f64, f64)> {
        assert_eq!(index, self.models);
        assert_eq!(self.models, self.lives);
        self.models += 1;
        Ok((100.0 + index as f64 * 0.05, 100.003 + index as f64 * 0.05))
    }
    fn live(&mut self, index: usize) -> Result<f64> {
        assert_eq!(index, self.lives);
        assert_eq!(self.models, self.lives + 1);
        self.lives += 1;
        Ok(100.025 + index as f64 * 0.05)
    }
}
#[test]
fn actual_native_driver_frame_loop_uses_one_session_one_request_per_frame() {
    let temp = tempfile::tempdir().unwrap();
    let mut client = client(temp.path(), "normal");
    let pid = client.pid();
    let mut frames = Frames {
        models: 0,
        lives: 0,
        waits: Vec::new(),
    };
    let result = frame_loop::drive40(&mut client, &mut frames, 100.05);
    let timing = client.finish(result).unwrap();
    assert_eq!(timing.len(), 40);
    assert_eq!(
        (frames.models, frames.lives, frames.waits.len()),
        (40, 40, 80)
    );
    assert_eq!(client.pid(), pid);
    assert_eq!(client.requests(), 40);
    assert_eq!(client.status(), Some(0));
    assert!(timing
        .iter()
        .all(|row| row.target.to_bits() == 0.0_f64.to_bits()));
}

#[test]
fn absent_native_setting_preserves_signed_zero_and_nan_without_process_or_stderr() {
    let temp = tempfile::tempdir().unwrap();
    let stderr = temp.path().join("stderr.log");
    let mut client = SchedulerClient::canonical(None, &stderr).unwrap();
    for (case, expected) in [
        ([-0.0, -0.05, -0.05], (-0.0_f64).to_bits()),
        (
            [f64::from_bits(0x7ff8_0000_0000_0042), 100.0, 100.08],
            0x7ff8_0000_0000_0042,
        ),
    ] {
        assert_eq!(
            client.target(case[0], case[1], case[2]).unwrap().to_bits(),
            expected
        );
    }
    client.finish(Ok(())).unwrap();
    assert!(client.pid().is_none());
    assert!(!stderr.exists());
    assert_eq!(client.requests(), 0);
}

#[test]
fn persistent_child_serves_forty_requests_and_is_reaped_on_eof() {
    let temp = tempfile::tempdir().unwrap();
    let mut client = client(temp.path(), "normal");
    let pid = client.pid();
    for index in 0..40 {
        assert_eq!(
            client
                .target(100.05 + index as f64 * 0.05, 100.0, 100.003)
                .unwrap(),
            0.0
        );
        assert_eq!(client.pid(), pid);
    }
    client.finish(Ok(())).unwrap();
    assert_eq!(client.status(), Some(0));
    assert!(client.is_closed());
    assert_eq!(
        fs::read_to_string(temp.path().join("requests.tsv"))
            .unwrap()
            .lines()
            .count(),
        40
    );
}

#[test]
fn empty_setting_is_an_error_instead_of_direct_policy() {
    let temp = tempfile::tempdir().unwrap();
    let stderr = temp.path().join("stderr.log");
    assert!(SchedulerClient::canonical(Some(Path::new("")), &stderr).is_err());
    assert!(!stderr.exists());
}

#[test]
fn spawn_failure_never_falls_back_to_direct_policy() {
    let temp = tempfile::tempdir().unwrap();
    assert!(SchedulerClient::canonical(
        Some(&temp.path().join("missing")),
        &temp.path().join("stderr.log")
    )
    .is_err());
}

#[test]
fn eof_exit_and_invalid_response_fail_closed_and_reap() {
    for mode in [
        "eof",
        "exit42",
        "malformed",
        "nonfinite",
        "long",
        "multiple",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let mut client = client(temp.path(), mode);
        let result = client.target(100.05, 100.0, 100.003);
        assert!(result.is_err(), "{mode}");
        assert!(client.failed());
        assert!(client.finish(result).is_err());
        assert!(client.status().is_some());
        assert!(client.is_closed());
    }
}

#[test]
fn one_request_has_a_shared_two_second_write_read_deadline() {
    let temp = tempfile::tempdir().unwrap();
    let mut client = client(temp.path(), "timeout");
    let start = Instant::now();
    let result = client.target(100.05, 100.0, 100.003);
    assert!(result
        .as_ref()
        .unwrap_err()
        .downcast_ref::<std::io::Error>()
        .is_some_and(|error| error.kind() == std::io::ErrorKind::TimedOut));
    assert!(start.elapsed() >= Duration::from_secs(2));
    assert!(client.finish(result).is_err());
    assert!(client.status().is_some());
}

#[test]
fn caught_eof_latches_failure_and_disallows_retry_and_successful_finish() {
    let temp = tempfile::tempdir().unwrap();
    let mut client = client(temp.path(), "eof");
    assert!(client.target(100.05, 100.0, 100.003).is_err());
    let requests = client.requests();
    assert!(client.target(100.05, 100.0, 100.003).is_err());
    assert_eq!(client.requests(), requests);
    assert!(client.finish(Ok(())).is_err());
    assert_eq!(client.status(), Some(0));
}

#[test]
fn abnormal_close42_is_not_silently_accepted() {
    let temp = tempfile::tempdir().unwrap();
    let mut client = client(temp.path(), "close42");
    assert_eq!(client.target(100.05, 100.0, 100.003).unwrap(), 0.0);
    assert!(client
        .finish(Ok(()))
        .unwrap_err()
        .to_string()
        .contains("42"));
    assert_eq!(client.status(), Some(42));
}

#[test]
fn normal_close_escalates_term_kill_wait_and_reports_failure() {
    let temp = tempfile::tempdir().unwrap();
    let mut client = client(temp.path(), "ignore-term");
    assert_eq!(client.target(100.05, 100.0, 100.003).unwrap(), 0.0);
    assert!(client.finish(Ok(())).is_err());
    assert_eq!(client.status(), Some(-9));
    assert!(client.is_closed());
}

#[test]
fn exceptional_close_reaps_stubborn_child_and_preserves_parent_error() {
    let temp = tempfile::tempdir().unwrap();
    let mut client = client(temp.path(), "ignore-term");
    assert_eq!(client.target(100.05, 100.0, 100.003).unwrap(), 0.0);
    let result: Result<()> = Err("parent run failed".into());
    assert_eq!(
        client.finish(result).unwrap_err().to_string(),
        "parent run failed"
    );
    assert_eq!(client.status(), Some(-9));
}

#[test]
fn real_native_binary_serves_four_sessions_160_independent_frozen_targets() {
    let bytes = include_bytes!("fixtures/planner-controller-original/scheduler-finite.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "0542466a70770f8a04c9c81893ecfe4c3252daf00b2935eeba5c3c6a4305a7bb"
    );
    let original: Value = serde_json::from_slice(bytes).unwrap();
    let cases = original["cases"]["timestamps"].as_array().unwrap();
    let expected = original["original_result"].as_array().unwrap();
    assert_eq!(cases.len(), 76);
    assert_eq!(expected.len(), 76);
    let temp = tempfile::tempdir().unwrap();
    let evidence = temp.path().join("scheduler.tsv");
    let mut processes = Vec::new();
    for run in 0..4 {
        let mut client = SchedulerClient::canonical_recorded(
            Some(Path::new(env!("CARGO_BIN_EXE_check-plannerd-ci"))),
            &temp.path().join(format!("stderr-{run}.log")),
            Some(&evidence),
        )
        .unwrap();
        processes.push(client.pid().unwrap());
        for index in (72..76).chain(0..36) {
            let case = cases[index]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| f64::from_bits(value.as_u64().unwrap()))
                .collect::<Vec<_>>();
            assert_eq!(
                client.target(case[0], case[1], case[2]).unwrap().to_bits(),
                expected[index].as_u64().unwrap()
            );
        }
        client.finish(Ok(())).unwrap();
        assert_eq!(client.requests(), 40);
        assert_eq!(client.status(), Some(0));
    }
    assert_eq!(processes.len(), 4);
    scheduler::verify_evidence(&evidence, 160).unwrap();
}

fn parser_observations() -> Value {
    let raw = include_bytes!("fixtures/planner-scheduler-original/observations.json");
    assert_eq!(raw.len(), 9160);
    assert_eq!(
        format!("{:x}", Sha256::digest(raw)),
        "afe5ea47497bf9ea1af07231400055f348c3c8e555020c1f747c97eb51168acd"
    );
    let observations: Value = serde_json::from_slice(raw).unwrap();
    assert_eq!(observations["schema_version"], 1);
    assert_eq!(
        observations["kind"],
        "independent-original-planner-parser-stage"
    );
    assert_eq!(
        observations["github_sha"],
        "aa79c7cd21a4b9bbb6bfb4c68ded9e857511f1cf"
    );
    assert_eq!(
        observations["source_sha256"],
        "a4aec1991a6751af60f5616698e75e0861b0a1f8b1c478e5382889681598c8dc"
    );
    assert_eq!(
        observations["scheduler_class_sha256"],
        "818e06d38749c45bd18692538dfc6f2815c4052a820bc3d9449f151e56b7f124"
    );
    assert_eq!(observations["cases"].as_array().unwrap().len(), 35);
    observations
}
fn line_bytes(case: &Value) -> Vec<u8> {
    assert_eq!(case["line_type"], "bytearray");
    let hex = case["line_hex"].as_str().unwrap();
    assert!(hex.is_ascii() && hex.len().is_multiple_of(2));
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}

#[test]
fn frozen_original_ascii_parser_stage_matches_native_bits_and_error_classes() {
    let original = parser_observations();
    let mut counts = [0; 4];
    for case in original["cases"].as_array().unwrap() {
        let result = parse_response(&line_bytes(case));
        if let Some(bits) = case["result_bits_be_hex"].as_str() {
            assert!(case["exception_type"].is_null());
            assert_eq!(
                result
                    .unwrap_or_else(|error| panic!("{}: {error}", case["name"]))
                    .to_bits(),
                u64::from_str_radix(bits, 16).unwrap(),
                "{}",
                case["name"]
            );
            counts[0] += 1;
        } else {
            let error = result.unwrap_err();
            let (category, count) = match &error {
                ResponseError::NonFinite => ("RuntimeError", 1),
                ResponseError::InvalidFloat => ("ValueError", 2),
                ResponseError::NonAscii => ("UnicodeDecodeError", 3),
            };
            assert_eq!(case["exception_type"], category, "{}", case["name"]);
            if error == ResponseError::NonFinite {
                assert_eq!(
                    error.to_string(),
                    case["exception_message"].as_str().unwrap()
                );
            }
            counts[count] += 1;
        }
    }
    assert_eq!(counts, [17, 7, 9, 2]);
}

#[test]
fn persistent_client_accepts_frozen_original_ascii_responses_in_one_session() {
    let original = parser_observations();
    let temp = tempfile::tempdir().unwrap();
    let mut client = client(temp.path(), "observed");
    let pid = client.pid();
    for case in original["cases"].as_array().unwrap() {
        let Some(bits) = case["result_bits_be_hex"].as_str() else {
            continue;
        };
        let line = line_bytes(case);
        assert!(
            !line.contains(&b'\n'),
            "this test owns one response line; AST-stage framing is separate"
        );
        fs::write(temp.path().join("response.line"), line).unwrap();
        assert_eq!(
            client.target(100.05, 100.0, 100.003).unwrap().to_bits(),
            u64::from_str_radix(bits, 16).unwrap(),
            "{}",
            case["name"]
        );
        assert_eq!(client.pid(), pid);
    }
    client.finish(Ok(())).unwrap();
    assert_eq!(client.requests(), 17);
    assert_eq!(client.status(), Some(0));
    assert!(client.is_closed());
}
