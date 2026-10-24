#![cfg(target_os = "linux")]
use openpilot_dev_tools::loggerd_check::io::{
    check_indices, check_remux_diagnostics, raw_packet, trace,
};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
const CHECKER: &str = env!("CARGO_BIN_EXE_check-loggerd-io");
const FIXTURE: &str = env!("CARGO_BIN_EXE_loggerd-lifecycle-fixture");
fn full(pid: u32, requested: usize) -> String {
    format!("{pid} write(9</dev/full<char 1:7>>, \"XXX\", {requested}) = -1 ENOSPC (No space left on device)\n")
}
fn raw_trace(pid: u32) -> String {
    full(pid, 4096)
        + &format!("{pid} close(9</dev/full<char 1:7>>) = 0\n{pid} +++ exited with 0 +++\n")
}
#[test]
fn raw_trace_requires_real_enospc_and_one_close_before_successful_actor_exit() {
    let good = raw_trace(123);
    let root = Path::new("/owned");
    let source = trace::audit(&good, root, "route", "raw").unwrap();
    assert_eq!(
        source,
        trace::audit(&raw_trace(987), root, "route", "raw").unwrap()
    );
    for bad in [
        good.replace("ENOSPC", "EIO"),
        good.replace("= -1 ENOSPC", "= 4096 ENOSPC"),
        good.replace("close(9</dev/full<char 1:7>>) = 0\n", ""),
        good.replace("exited with 0", "exited with 1"),
        good.replace("/dev/full", "/dev/full-foreign"),
        good.replace(
            "close(9</dev/full<char 1:7>>) = 0",
            "close(9</dev/full<char 1:7>>) = -1 EINTR (Interrupted)",
        ),
        good.replace(
            "+++ exited",
            "write(9</dev/full<char 1:7>>, \"X\", 1) = -1 ENOSPC (full)\n123 +++ exited",
        ),
    ] {
        assert!(trace::audit(&bad, root, "route", "raw").is_err(), "{bad}");
    }
    let changed = trace::audit(&good.replace("4096", "4095"), root, "route", "raw").unwrap();
    assert_ne!(
        changed, source,
        "write length must survive source/native comparison"
    );
}
#[test]
fn unfinished_calls_rejoin_per_actor_and_reject_orphans_or_wrong_resumes() {
    let split = "8 write(4</dev/full<char 1:7>>, \"XX\", 2 <unfinished ...>\n9 close(5</etc/hosts>) = 0\n8 <... write resumed>) = -1 ENOSPC (full)\n8 close(4</dev/full<char 1:7>>) = 0\n8 +++ exited with 0 +++\n";
    assert!(trace::audit(split, Path::new("/owned"), "route", "raw").is_ok());
    assert!(trace::parse(&split.replace("<... write resumed>", "<... close resumed>")).is_err());
    assert!(trace::parse("8 <... write resumed>) = -1 ENOSPC (full)\n").is_err());
    assert!(trace::parse("8 write(4</dev/full>, \"X\", 1 <unfinished ...>\n").is_err());
}
#[test]
fn flush_trace_has_params_positive_control_no_log_sync_and_exact_raw_write_close() {
    let good = "3 fsync(4</owned/params/private>) = 0\n3 write(8</owned/logs/route--0/fcamera.hevc>, \"fixture-headerfixture-payload\", 29) = 29\n3 close(8</owned/logs/route--0/fcamera.hevc>) = 0\n3 +++ exited with 0 +++\n";
    assert!(trace::audit(good, Path::new("/owned"), "route", "flush").is_ok());
    for bad in [
        good.replace("fsync(4<", "fdatasync(4<"),
        good.replace("/params/", "/foreign/"),
        good.replace(", 29) = 29", ", 28) = 28"),
        good.replace(
            "3 +++",
            "3 fdatasync(8</owned/logs/route--0/rlog.zst>) = 0\n3 +++",
        ),
    ] {
        assert!(trace::audit(&bad, Path::new("/owned"), "route", "flush").is_err());
    }
}
#[test]
fn remux_trace_requires_enospc_then_real_rotated_output_write_and_close() {
    let old = full(7, 32768) + "7 close(9</dev/full<char 1:7>>) = 0\n";
    let next = "7 write(9</owned/logs/route--1/qcamera.ts>, \"packets\", 188) = 188\n7 close(9</owned/logs/route--1/qcamera.ts>) = 0\n";
    let good = old.clone() + next + "7 +++ exited with 0 +++\n";
    assert!(trace::audit(&good, Path::new("/owned"), "route", "remux").is_ok());
    let reordered = next.to_owned() + &old + "7 +++ exited with 0 +++\n";
    assert!(trace::audit(&reordered, Path::new("/owned"), "route", "remux").is_err());
    assert!(trace::audit(
        &(old + "7 +++ exited with 0 +++\n"),
        Path::new("/owned"),
        "route",
        "remux"
    )
    .is_err());
}
fn routes() -> Vec<Value> {
    let mut first = (0..640)
        .map(|frame| json!({"qRoadEncodeIdx":{"frameId":frame,"flags":8}}))
        .collect::<Vec<_>>();
    first.push(json!({"sentinel":{"type":"endOfSegment"}}));
    vec![
        json!({"rlog":first}),
        json!({"rlog":[{"sentinel":{"type":"startOfSegment"}},{"qRoadEncodeIdx":{"frameId":640}},{"qRoadEncodeIdx":{"frameId":641}}]}),
    ]
}
#[test]
fn indices_require_all_three_raw_frames_and_all_640_plus_rotation() {
    let good = routes();
    assert!(check_indices(&good, "remux").is_ok());
    let mut bad = good.clone();
    bad[0]["rlog"].as_array_mut().unwrap().remove(320);
    assert!(check_indices(&bad, "remux").is_err());
    let mut bad = good.clone();
    bad[0]["rlog"][320] = bad[0]["rlog"][319].clone();
    assert!(check_indices(&bad, "remux").is_err());
    let mut bad = good.clone();
    bad[1]["rlog"][2]["qRoadEncodeIdx"]["frameId"] = json!(642);
    assert!(check_indices(&bad, "remux").is_err());
    assert!(check_indices(&good[..1], "remux").is_err());
    let good = vec![
        json!({"rlog":[{"roadEncodeIdx":{"frameId":0}},{"roadEncodeIdx":{"frameId":1}},{"roadEncodeIdx":{"frameId":2}}]}),
    ];
    assert!(check_indices(&good, "raw").is_ok());
    let mut bad = good;
    bad[0]["rlog"][2]["roadEncodeIdx"]["frameId"] = json!(1);
    assert!(check_indices(&bad, "raw").is_err());
}
#[test]
fn required_remux_diagnostics_keep_level_and_exact_errno() {
    let good = vec![
        json!({"levelnum":30,"msg":"ts encoder write issue len: 99 ts: 100"}),
        json!({"levelnum":40,"msg":"av_write_trailer failed -28"}),
        json!({"levelnum":40,"msg":"avio_closep failed -28"}),
    ];
    assert!(check_remux_diagnostics(&good).is_ok());
    for i in 0..3 {
        let mut bad = good.clone();
        bad.remove(i);
        assert!(check_remux_diagnostics(&bad).is_err());
    }
    let mut bad = good;
    bad[1]["levelnum"] = json!(30);
    assert!(check_remux_diagnostics(&bad).is_err());
}
#[test]
fn cli_help_arity_and_occupied_output_do_not_clobber_foreign_evidence() {
    assert!(Command::new(CHECKER)
        .arg("--help")
        .status()
        .unwrap()
        .success());
    assert_eq!(
        Command::new(CHECKER)
            .arg("--binary")
            .output()
            .unwrap()
            .status
            .code(),
        Some(2)
    );
    let t = tempfile::tempdir().unwrap();
    let out = t.path().join("occupied");
    fs::create_dir(&out).unwrap();
    fs::write(out.join("foreign"), b"unchanged").unwrap();
    let r = Command::new(CHECKER)
        .args([
            "--binary",
            FIXTURE,
            "--original",
            FIXTURE,
            "--mode",
            "flush",
            "--output",
        ])
        .arg(&out)
        .output()
        .unwrap();
    assert_eq!(r.status.code(), Some(1));
    assert_eq!(fs::read(out.join("foreign")).unwrap(), b"unchanged");
    assert!(!out.join("control").exists());
    for extra in [
        vec!["--mode", "unknown"],
        vec!["--mode", "raw", "--asan-remux"],
        vec!["--remux-only", "--asan-remux"],
    ] {
        let unused = t.path().join("unused");
        let r = Command::new(CHECKER)
            .args(["--binary", FIXTURE, "--original", FIXTURE, "--output"])
            .arg(&unused)
            .args(extra)
            .env_remove("ASAN_OPTIONS")
            .output()
            .unwrap();
        assert_eq!(r.status.code(), Some(2));
        assert!(!unused.exists());
    }
}

struct OwnedChecker(std::process::Child);
impl Drop for OwnedChecker {
    fn drop(&mut self) {
        use rustix::process::{kill_process, Pid, Signal};
        if self.0.try_wait().ok().flatten().is_some() {
            return;
        }
        let _ = kill_process(Pid::from_child(&self.0), Signal::TERM);
        let until = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while std::time::Instant::now() < until {
            if self.0.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[test]
fn actual_io_worker_cancellation_reaps_detached_trace_and_escaped_peers_before_ipc() {
    use rustix::process::{pidfd_open, pidfd_send_signal, Pid, PidfdFlags, Signal};
    use std::{
        process::Stdio,
        time::{Duration, Instant},
    };
    for signal in [Signal::INT, Signal::TERM] {
        let parent = std::env::var_os("LOGGERD_IO_LIFECYCLE_EVIDENCE")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        fs::create_dir_all(&parent).unwrap();
        let t = tempfile::Builder::new()
            .prefix("loggerd-io-cancel-")
            .tempdir_in(&parent)
            .unwrap()
            .keep();
        let records = t.join("records");
        let output = t.join("output");
        let child = Command::new(CHECKER)
            .args([
                "--binary",
                FIXTURE,
                "--original",
                FIXTURE,
                "--mode",
                "flush",
                "--output",
            ])
            .arg(&output)
            .env("LOGGERD_LIFECYCLE_RECORDS", &records)
            .env("LOGGERD_LIFECYCLE_MODE", "hold")
            .stdout(Stdio::null())
            .stderr(fs::File::create(t.join("checker.stderr")).unwrap())
            .spawn()
            .unwrap();
        let mut checker = OwnedChecker(child);
        let id = pidfd_open(Pid::from_child(&checker.0), PidfdFlags::empty()).unwrap();
        let until = Instant::now() + Duration::from_secs(20);
        while !records.join("leaf.json").is_file() || !records.join("paths.json").is_file() {
            assert!(
                checker.0.try_wait().unwrap().is_none() && Instant::now() < until,
                "IO fixture did not start: {}",
                fs::read_to_string(t.join("checker.stderr")).unwrap()
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut peers = Vec::new();
        for role in ["logger", "escaped", "leaf"] {
            let row: Value =
                serde_json::from_slice(&fs::read(records.join(format!("{role}.json"))).unwrap())
                    .unwrap();
            peers.push(row["pid"].as_i64().unwrap() as i32);
        }
        let paths: Value =
            serde_json::from_slice(&fs::read(records.join("paths.json")).unwrap()).unwrap();
        let socket = PathBuf::from(paths["socket"].as_str().unwrap());
        fs::remove_file(&socket).unwrap();
        fs::write(&socket, b"foreign replacement").unwrap();
        pidfd_send_signal(&id, signal).unwrap();
        let until = Instant::now() + Duration::from_secs(20);
        let status = loop {
            if let Some(status) = checker.0.try_wait().unwrap() {
                break status;
            }
            assert!(
                Instant::now() < until,
                "IO cancellation exceeded cleanup bound"
            );
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(status.code(), Some(128 + signal.as_raw()));
        for pid in peers {
            assert!(
                !Path::new(&format!("/proc/{pid}")).exists(),
                "unreaped IO descendant {pid}"
            );
        }
        assert!(!Path::new(paths["namespace"].as_str().unwrap()).exists());
        assert_eq!(fs::read(&socket).unwrap(), b"foreign replacement");
        let cleanup: Value =
            serde_json::from_slice(&fs::read(output.join("control/cleanup.json")).unwrap())
                .unwrap();
        assert_eq!(cleanup["processes_confirmed"], true);
        assert!(cleanup["error"].is_null());
        assert_eq!(cleanup["pending_receivers"], 0);
        assert_eq!(cleanup["pending_namespaces"], 0);
        let events = cleanup["events"].as_array().unwrap();
        let removed = events
            .iter()
            .position(|v| v["phase"] == "ipc-cleanup")
            .unwrap();
        assert!(events[..removed]
            .iter()
            .any(|v| v["phase"] == "worker-reaped"));
        assert!(events[..removed]
            .iter()
            .any(|v| v["phase"] == "adopted-children-empty"));
        fs::remove_file(socket).unwrap();
    }
}

fn decode(bytes: &[u8]) -> Value {
    let compressed = zstd::stream::encode_all(bytes, 1).unwrap();
    let mut value = openpilot_dev_tools::bootlog_artifact_check::decode_zstd(&compressed)
        .unwrap()
        .1;
    for row in value.as_array_mut().unwrap() {
        assert!(row["logMonoTime"].as_u64().unwrap() > 0);
        row.as_object_mut().unwrap().remove("logMonoTime");
    }
    value
}
#[test]
#[ignore = "hosted only; authenticated artifact12432287942, inert records only"]
fn independent_original_packets_and_raw_syscalls_replay_without_python() {
    let root = PathBuf::from(
        std::env::var_os("LOGGERD_IO_ORIGINAL_CAPTURE").expect("sealed independent inputs"),
    );
    assert_eq!(
        fs::read_to_string(root.join("archive.sha256"))
            .unwrap()
            .trim(),
        "3655dfbe1524303c7927bcdd0ede43599e75094fe787f0ec80d84a82cba3a516"
    );
    for (case, frames) in [
        ("logger-flush/original", vec![None]),
        ("logger-io/original", vec![Some(0), Some(1), Some(2)]),
    ] {
        let parent = root.join(case);
        let index: Vec<Value> =
            serde_json::from_slice(&fs::read(parent.join("inputs/index.json")).unwrap()).unwrap();
        let packets = index
            .iter()
            .filter(|r| r["service"] == "roadEncodeData")
            .collect::<Vec<_>>();
        assert_eq!(packets.len(), frames.len());
        for (row, frame) in packets.iter().zip(frames) {
            let path = row["path"].as_str().unwrap();
            assert!(!path.contains('/') && !path.contains(".."));
            let source = fs::read(parent.join("inputs").join(path)).unwrap();
            use sha2::{Digest, Sha256};
            assert_eq!(row["sha256"], format!("{:x}", Sha256::digest(&source)));
            assert_eq!(
                decode(&raw_packet(frame).unwrap()),
                decode(&source),
                "{case}/{path}: full original Event except ambient logMonoTime"
            );
        }
    }
    let a = fs::read_to_string(root.join("logger-io/original/writes.log")).unwrap();
    let b = fs::read_to_string(root.join("logger-io/rust/writes.log")).unwrap();
    assert_eq!(
        trace::audit(&a, Path::new("/unused"), "route", "raw").unwrap(),
        trace::audit(&b, Path::new("/unused"), "route", "raw").unwrap()
    );
    // Historical flush captured only sync calls: never relabel it as write/close
    // evidence. The new current-source hosted controller must collect that.
    let old = fs::read_to_string(root.join("logger-flush/original/sync-syscalls.log")).unwrap();
    let parsed = trace::parse(&old).unwrap();
    assert!(parsed
        .calls
        .iter()
        .any(|c| c.operation == "fsync" && c.path.contains("/params/")));
    assert!(!parsed.calls.iter().any(|c| c.path.contains("/logs/")));
    assert!(trace::audit(&old, Path::new("/unused"), "route", "flush").is_err());
}
