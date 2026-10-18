//! Real owned shell acknowledgements, not a timing guess or candidate oracle.
use super::*;
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    process::{pidfd_open, pidfd_send_signal, Pid, PidfdFlags, Signal},
};
use std::{os::fd::OwnedFd, thread};
struct Identities(Vec<OwnedFd>);
impl Drop for Identities {
    fn drop(&mut self) {
        for id in &self.0 {
            let _ = pidfd_send_signal(id, Signal::KILL);
        }
    }
}
fn exited(fd: &OwnedFd) -> bool {
    poll(
        &mut [PollFd::new(fd, PollFlags::IN)],
        Some(&Timespec::default()),
    )
    .unwrap()
        != 0
}
fn identity(path: &Path) -> Option<OwnedFd> {
    let pid = Pid::from_raw(fs::read_to_string(path).ok()?.trim().parse().ok()?)?;
    let id = pidfd_open(pid, PidfdFlags::empty()).ok()?;
    let mut tag = b"FIXTURE_PID=".to_vec();
    tag.extend_from_slice(path.as_os_str().as_encoded_bytes());
    let env = fs::read(format!("/proc/{}/environ", pid.as_raw_pid())).ok()?;
    (!exited(&id) && env.split(|b| *b == 0).any(|v| v == tag)).then_some(id)
}
#[test]
fn native_retrace_cancellation_cleans_source_and_separate_candidate_groups() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let output = tempfile::tempdir().unwrap();
    let options = Options {
        binary: fs::canonicalize(
            std::env::var_os("TOMBSTONE_PROBE_BINARY")
                .expect("current verified tombstone_trace required"),
        )
        .unwrap(),
        params_binding: PathBuf::from("unused-binding-provenance"),
        output: output.path().to_owned(),
        runner: vec![],
        environment: std::env::vars_os().collect(),
    };
    let cancel = Cancellation::new().unwrap();
    let mut pair = Pair::new(&root, &options, output.path(), &cancel).unwrap();
    pair.write(
        Path::new("apport/cancel.crash"),
        schedule::BODY.as_bytes(),
        Some(0o640),
    )
    .unwrap();
    pair.write(Path::new("mode"), b"timeout", None).unwrap();
    let paths = pair.paths(Path::new("helper.pid"));
    let flag = cancel.flag();
    let (result, ids, after) = thread::scope(|scope| {
        let worker = scope.spawn(|| {
            let end = Instant::now() + Duration::from_secs(5);
            let mut ids = Identities(Vec::new());
            for path in &paths {
                loop {
                    if let Some(fd) = identity(path) {
                        ids.0.push(fd);
                        break;
                    }
                    if Instant::now() >= end {
                        flag.store(true, Ordering::Relaxed);
                        panic!("actual helper did not acknowledge: {}", path.display());
                    }
                    thread::sleep(Duration::from_millis(5));
                }
            }
            let at = Instant::now();
            flag.store(true, Ordering::Relaxed);
            (ids, at)
        });
        let result = pair.call(
            &scalar(r#"{"op":"retrace","path":"$FIXTURE/apport/cancel.crash","timeout_ms":30000}"#),
            false,
        );
        let (ids, at) = worker.join().unwrap();
        (result, ids, at)
    });
    assert!(result.is_err(), "cancellation became ordinary source JSON");
    drop(pair);
    assert!(
        after.elapsed() < Duration::from_secs(3),
        "cleanup exceeded cancellation bound"
    );
    let end = Instant::now() + Duration::from_secs(1);
    while ids.0.iter().any(|fd| !exited(fd)) && Instant::now() < end {
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        ids.0.iter().all(exited),
        "owned source/native retrace survived cancellation"
    );
    // This API cancellation never fabricates an OS signal to re-raise.
    assert!(cancel.signal().is_none());
}
#[test]
fn original_normalization_is_limited_and_raw_value_json_is_not_mutated() {
    let root = Path::new("/owned/root");
    let raw = scalar(
        r#"{"created":1,"path":"/owned/root/native/a","value_json":"{\"s\":\"\\udcff\",\"big\":99999999999999999999999999999}","nested":{"created":2,"x":true}}"#,
    );
    let normalized = normalize(&raw, root).unwrap();
    assert!(equal(
        &normalized,
        &scalar(
            r#"{"path":"$FIXTURE/a","value_json":{"s":"\udcff","big":99999999999999999999999999999},"nested":{"x":true}}"#
        )
    ));
    assert!(matches!(
        raw.get("value_json").unwrap().view(),
        JsonView::Text(_)
    ));
    assert!(!equal(
        &normalized,
        &scalar(
            r#"{"path":"$FIXTURE/a","value_json":{"s":"�","big":99999999999999999999999999999},"nested":{"x":true}}"#
        )
    ));
}
