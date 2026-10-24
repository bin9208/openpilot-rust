//! Actual private helper processes prove abort/deadline ownership; no oracle or
//! candidate process supplies expected source policy answers.
use super::Session;
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    process::{pidfd_open, pidfd_send_signal, Pid, PidfdFlags, Signal},
};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    os::{fd::OwnedFd, unix::fs::PermissionsExt},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

struct Cleanup {
    cancel: Arc<AtomicBool>,
    ids: Vec<OwnedFd>,
}
impl Drop for Cleanup {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        for id in &self.ids {
            let _ = pidfd_send_signal(id, Signal::KILL);
        }
    }
}
fn finished(id: &OwnedFd) -> bool {
    poll(
        &mut [PollFd::new(id, PollFlags::IN)],
        Some(&Timespec::default()),
    )
    .unwrap()
        != 0
}
fn live_identity(path: &Path, token: &Path) -> Option<OwnedFd> {
    let pid = fs::read_to_string(path)
        .ok()?
        .trim()
        .parse::<i32>()
        .ok()
        .and_then(Pid::from_raw)?;
    let identity = pidfd_open(pid, PidfdFlags::empty()).ok()?;
    let expected = format!("TOMBSTONE_SOURCE_TEST_TOKEN={}", token.display());
    let env = fs::read(format!("/proc/{}/environ", pid.as_raw_pid())).ok()?;
    (!finished(&identity)
        && env
            .split(|byte| *byte == 0)
            .any(|entry| entry == expected.as_bytes()))
    .then_some(identity)
}
fn prepare(program: &str) -> (tempfile::TempDir, Session, Vec<u8>) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir(root.join("bin")).unwrap();
    let helper = root.join("bin/apport-retrace");
    fs::write(&helper, program).unwrap();
    fs::set_permissions(helper, fs::Permissions::from_mode(0o755)).unwrap();
    let input = root.join("input.crash");
    fs::write(&input, "fixture\n").unwrap();
    let mut env = std::env::vars_os().collect::<BTreeMap<OsString, OsString>>();
    env.insert(
        "PATH".into(),
        format!("{}:/usr/bin:/bin", root.join("bin").display()).into(),
    );
    env.insert(
        "TOMBSTONE_SOURCE_TEST_TOKEN".into(),
        root.as_os_str().to_owned(),
    );
    let request =
        serde_json::to_vec(&serde_json::json!({"op":"retrace","path":input,"timeout_ms":30000}))
            .unwrap();
    let session = Session::new(root, &env).unwrap();
    (temp, session, request)
}
const SLOW:&str="#!/bin/sh\necho $$ > \"$TOMBSTONE_SOURCE_TEST_TOKEN/helper.pid\"\nsleep 30 &\necho $! > \"$TOMBSTONE_SOURCE_TEST_TOKEN/sleeper.pid\"\nprintf acknowledged > \"$TOMBSTONE_SOURCE_TEST_TOKEN/ack\"\nwait\n";
fn acknowledge(root: &Path, guard: &mut Cleanup) {
    let until = Instant::now() + Duration::from_secs(3);
    loop {
        if root.join("ack").is_file() {
            if let (Some(helper), Some(sleeper)) = (
                live_identity(&root.join("helper.pid"), root),
                live_identity(&root.join("sleeper.pid"), root),
            ) {
                guard.ids.extend([helper, sleeper]);
                return;
            }
        }
        assert!(
            Instant::now() < until,
            "owned source helper and sleeper never acknowledged live"
        );
        thread::sleep(Duration::from_millis(5));
    }
}
fn assert_cleaned(guard: &Cleanup) {
    let until = Instant::now() + Duration::from_secs(2);
    while guard.ids.iter().any(|id| !finished(id)) && Instant::now() < until {
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        guard.ids.iter().all(finished),
        "source helper survived controlled cleanup; test cleanup follows this assertion"
    );
}
#[test]
fn request_cancel_after_acknowledged_live_helper_and_sleeper_is_fatal_and_cleans_both() {
    let (temp, mut source, request) = prepare(SLOW);
    let cancel = Arc::new(AtomicBool::new(false));
    thread::scope(|scope| {
        // Drops before the scope joins even if any assertion panics.
        let mut cleanup = Cleanup {
            cancel: Arc::clone(&cancel),
            ids: Vec::new(),
        };
        let flag = Arc::clone(&cancel);
        let worker = scope.spawn(move || {
            source.request_controlled(&request, Instant::now() + Duration::from_secs(10), flag)
        });
        acknowledge(temp.path(), &mut cleanup);
        let started = Instant::now();
        cancel.store(true, Ordering::Relaxed);
        let error = worker
            .join()
            .unwrap()
            .err()
            .expect("cancellation was serialized as ordinary source success");
        assert_eq!(error.kind(), "InterruptedError");
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "source cancellation waited for original30sec helper"
        );
        assert_cleaned(&cleanup);
    });
}
#[test]
fn whole_request_deadline_is_fatal_before_original_30second_timeout_and_cleans_helpers() {
    let (temp, mut source, request) = prepare(SLOW);
    let cancel = Arc::new(AtomicBool::new(false));
    thread::scope(|scope| {
        let mut cleanup = Cleanup {
            cancel: Arc::clone(&cancel),
            ids: Vec::new(),
        };
        let flag = Arc::clone(&cancel);
        let started = Instant::now();
        let worker = scope.spawn(move || {
            source.request_controlled(&request, started + Duration::from_secs(5), flag)
        });
        acknowledge(temp.path(), &mut cleanup);
        let error = worker
            .join()
            .unwrap()
            .err()
            .expect("controller deadline became ordinary source timeout string");
        assert_eq!(error.kind(), "TimeoutError");
        assert!(started.elapsed() < Duration::from_secs(7));
        assert_cleaned(&cleanup);
    });
}
#[test]
fn helper_stderr_streams_into_owned_source_file_and_control_resets_for_plain_requests() {
    let (temp, mut source, request) = prepare(
        "#!/bin/sh\nprintf '%s\\n' \"$TOMBSTONE_SOURCE_TEST_TOKEN\" >&2\nprintf 'trace\\n'\n",
    );
    let path: PathBuf = temp.path().join("source.stderr");
    source.set_stderr(fs::File::create(&path).unwrap()).unwrap();
    let cancelled = Arc::new(AtomicBool::new(true));
    assert_eq!(
        source
            .request_controlled(&request, Instant::now() + Duration::from_secs(2), cancelled)
            .err()
            .unwrap()
            .kind(),
        "InterruptedError"
    );
    // The expired/cancelled request must not poison a later ordinary request.
    let result = source.request(&request).unwrap();
    assert!(result
        .get("result")
        .unwrap()
        .get("value")
        .unwrap()
        .text_eq("trace\n"));
    assert_eq!(
        fs::read_to_string(path).unwrap(),
        format!("{}\n", temp.path().display())
    );
}

#[test]
fn original_retrace_timeout_numbers_preserve_errors_and_spawn_error_precedence() {
    use sha2::{Digest, Sha256};
    let bytes =
        include_bytes!("../../tests/fixtures/tombstone-tools/timeout-boundaries-original.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "f4cabec1d644c83b881a6bdc810d6457afb5e22235660fe65415044b8b0a8669"
    );
    let capture: serde_json::Value = serde_json::from_slice(bytes).unwrap();
    assert_eq!(capture["candidate_executed"], false);
    assert_eq!(capture["cases"].as_array().unwrap().len(), 16);
    let mut mismatches = Vec::new();
    for case in capture["cases"].as_array().unwrap() {
        let (temp, mut source, _) = prepare(SLOW);
        let shell = if case["shell"] == "$FIXTURE/missing-shell" {
            temp.path().join("missing-shell")
        } else {
            PathBuf::from("/bin/bash")
        };
        let path = serde_json::to_string(&temp.path().join("input.crash")).unwrap();
        let shell = serde_json::to_string(&shell).unwrap();
        let request = format!(
            "{{\"op\":\"retrace\",\"path\":{path},\"timeout_ms\":{},\"shell\":{shell}}}",
            case["timeout_json"].as_str().unwrap()
        );
        let actual = source.request(request.as_bytes()).unwrap();
        let actual: serde_json::Value = serde_json::from_str(&actual.to_json().unwrap()).unwrap();
        if actual != case["response"] {
            mismatches.push((case["label"].clone(), case["response"].clone(), actual));
        }
    }
    assert!(
        mismatches.is_empty(),
        "original timeout boundary mismatches: {mismatches:?}"
    );
}
