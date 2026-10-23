//! Runtime regressions for #270. This module also compiles against b55552dbb's pty.rs.
use super::{read, write, Generation};
use crate::terminal::{state::State, Config};
use rustix::process::{Pid, Signal};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};

fn identity(pid: i32) -> Option<(u64, char, i32, i32)> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let fields: Vec<_> = stat.rsplit_once(')')?.1.split_whitespace().collect();
    Some((
        fields.get(19)?.parse().ok()?,
        fields.first()?.chars().next()?,
        fields.get(2)?.parse().ok()?,
        fields.get(3)?.parse().ok()?,
    ))
}

struct Fixture {
    // Emergency cleanup also runs on a baseline RED assertion failure.
    members: BTreeMap<i32, u64>,
    unrelated: std::process::Child,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        for (&pid, &start) in &self.members {
            if identity(pid).is_some_and(|value| value.0 == start && value.1 != 'Z') {
                let _ = rustix::process::kill_process(Pid::from_raw(pid).unwrap(), Signal::KILL);
            }
        }
        let _ = self.unrelated.kill();
        let _ = self.unrelated.wait();
    }
}

fn config() -> Config {
    let directory = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    Config {
        launcher: directory.join("openpilot-process-child"),
        cli: directory.join("openpilot-carrot-command"),
        cli_cwd: PathBuf::from("/tmp"),
        start_dir: PathBuf::from("/tmp"),
        motd_dir: PathBuf::from("/nonexistent"),
        motd_cache: PathBuf::from("/nonexistent"),
        tmux_log: PathBuf::from("/nonexistent"),
        web_session: "unused-270".into(),
        capture_lines: 160,
    }
}

// This is a Rust raw string: printf needs one literal backslash before n.
// Start each record on a fresh line so a Bash prompt cannot hide the prefix.
const SETUP: &[u8] = br#"set -m
trap '' HUP
issue270_ready() { local role=$1 pid=$2 stat; local -a fields; IFS= read -r stat < "/proc/$pid/stat" || return; IFS=' ' read -r -a fields <<< "${stat##*) }"; printf '\nISSUE270_READY role=%s pid=%s pgid=%s sid=%s start=%s\n' "$role" "$pid" "${fields[2]}" "${fields[3]}" "${fields[19]}"; }
export -f issue270_ready
issue270_ready leader "$BASHPID"
/bin/bash --noprofile --norc -c 'trap "" HUP; sleep 600 & issue270_ready nested "$!"; wait' & issue270_ready job1 "$!"
sleep 600 & issue270_ready job2 "$!"
"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Ready {
    role: &'static str,
    pid: i32,
    group: i32,
    session: i32,
    start: u64,
}

fn ready_record(line: &[u8]) -> Option<Ready> {
    // A trailing fragment may contain only the first digits of a PID. Never
    // accept it, or echoed format placeholders, as process readiness.
    let line = line.strip_suffix(b"\n")?;
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    let fields: Vec<_> = std::str::from_utf8(line)
        .ok()?
        .split_ascii_whitespace()
        .collect();
    let ["ISSUE270_READY", role, pid, group, session, start] = fields.as_slice() else {
        return None;
    };
    let role = match *role {
        "role=leader" => "leader",
        "role=job1" => "job1",
        "role=job2" => "job2",
        "role=nested" => "nested",
        _ => return None,
    };
    let positive = |value: &str, prefix| {
        value
            .strip_prefix(prefix)?
            .parse::<i32>()
            .ok()
            .filter(|pid| *pid > 0)
    };
    Some(Ready {
        role,
        pid: positive(pid, "pid=")?,
        group: positive(group, "pgid=")?,
        session: positive(session, "sid=")?,
        start: start.strip_prefix("start=")?.parse().ok()?,
    })
}

#[test]
fn fixture_ready_records_require_complete_literal_identity() {
    let record = b"ISSUE270_READY role=job1 pid=123 pgid=123 sid=100 start=456\r\n";
    assert_eq!(
        ready_record(record),
        Some(Ready {
            role: "job1",
            pid: 123,
            group: 123,
            session: 100,
            start: 456,
        })
    );
    assert_eq!(ready_record(&record[..record.len() - 2]), None);
    assert_eq!(
        ready_record(br"ISSUE270_READY role=job1 pid=123 pgid=123 sid=100 start=456\n"),
        None
    );
    assert_eq!(
        ready_record(b"ISSUE270_READY role=job1 pid=%s pgid=%s sid=%s start=%s\r\n"),
        None
    );
    assert_eq!(
        ready_record(b"owned> ISSUE270_READY role=job1 pid=123 pgid=123 sid=100 start=456\r\n"),
        None
    );
    assert_eq!(
        ready_record(
            b"printf 'ISSUE270_READY role=job1 pid=%s pgid=%s sid=%s start=%s\\n' \"$!\"\r\n"
        ),
        None
    );
}

fn session_snapshot(leader: i32) -> Vec<serde_json::Value> {
    std::fs::read_dir("/proc")
        .unwrap()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let pid = entry.file_name().to_str()?.parse::<i32>().ok()?;
            let (start, state, group, session) = identity(pid)?;
            (session == leader).then(|| {
                serde_json::json!({
                    "pid": pid, "pgid": group, "sid": session, "start": start, "state": state,
                })
            })
        })
        .collect()
}

async fn owned_jobs() -> (Generation, Fixture) {
    let subreaper = rustix::process::child_subreaper().unwrap();
    let generation = Generation::start(&config(), "exec /bin/bash --noprofile --norc -i").unwrap();
    assert_eq!(rustix::process::child_subreaper().unwrap(), subreaper);
    let mut fixture = Fixture {
        members: BTreeMap::new(),
        unrelated: Command::new("/bin/sleep").arg("600").spawn().unwrap(),
    };
    let leader = generation.pid.as_raw_nonzero().get();
    eprintln!(
        "{}",
        serde_json::json!({
            "event": "issue270_fixture_tx", "leader": leader,
            "script": String::from_utf8_lossy(SETUP),
        })
    );
    write(generation.master.clone(), SETUP).await.unwrap();
    let mut output = Vec::new();
    let mut ready = BTreeMap::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    while ready.len() < 4 {
        let received =
            tokio::time::timeout_at(deadline.into(), read(generation.master.clone())).await;
        let bytes = match received {
            Ok(Ok(bytes)) if !bytes.is_empty() => bytes,
            error => {
                eprintln!(
                    "{}",
                    serde_json::json!({
                        "event": "issue270_fixture_setup_failed", "leader": leader,
                        "ready": format!("{ready:?}"), "read_result": format!("{error:?}"),
                        "transcript": String::from_utf8_lossy(&output),
                        "session": session_snapshot(leader),
                    })
                );
                panic!("fixture PTY readiness deadline/EOF/read error: {error:?}");
            }
        };
        eprintln!(
            "{}",
            serde_json::json!({
                "event": "issue270_fixture_rx", "leader": leader,
                "text": String::from_utf8_lossy(&bytes), "bytes": &bytes,
            })
        );
        output.extend_from_slice(&bytes);
        assert!(
            output.len() <= 65536,
            "fixture readiness exceeded capture limit"
        );
        for record in output
            .split_inclusive(|byte| *byte == b'\n')
            .filter_map(ready_record)
        {
            let (start, state, group, session) =
                identity(record.pid).expect("fixture readiness PID missing");
            assert_eq!(
                (start, group, session),
                (record.start, record.group, record.session),
                "fixture readiness identity differs from /proc: {record:?}"
            );
            assert_ne!(state, 'Z', "fixture readiness process is not live");
            assert_eq!(
                session, leader,
                "fixture readiness session differs from leader"
            );
            assert_eq!(
                record.role == "leader",
                record.pid == leader,
                "fixture readiness leader identity"
            );
            if let Some(previous) = ready.insert(record.role, record) {
                assert_eq!(previous, record, "fixture readiness identity changed");
            } else {
                eprintln!(
                    "{}",
                    serde_json::json!({
                        "event": "issue270_fixture_ready", "role": record.role,
                        "pid": record.pid, "pgid": group, "sid": session, "start": start, "state": state,
                    })
                );
            }
            if record.role != "leader" {
                fixture.members.insert(record.pid, start);
            }
        }
    }
    let groups: std::collections::BTreeSet<_> = fixture
        .members
        .keys()
        .map(|&pid| {
            let (_, state, group, session) = identity(pid).unwrap();
            assert_ne!(state, 'Z');
            assert_eq!(session, leader);
            assert_ne!(group, leader, "fixture must exercise job control");
            group
        })
        .collect();
    assert_eq!(
        fixture.members.len(),
        3,
        "fixture readiness requires three distinct jobs"
    );
    assert!(
        groups.len() >= 2,
        "fixture needs multiple job-control groups"
    );
    assert_eq!(
        ready["nested"].group, ready["job1"].group,
        "nested job must share its parent job group"
    );
    eprintln!(
        "{}",
        serde_json::json!({
            "event": "issue270_fixture_setup_complete", "leader": leader,
            "roles": ready.keys().collect::<Vec<_>>(), "groups": groups,
            "session": session_snapshot(leader),
        })
    );
    (generation, fixture)
}

async fn assert_cleaned(leader: i32, fixture: &mut Fixture) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let remaining: Vec<_> = fixture
            .members
            .iter()
            .filter(|(pid, start)| identity(**pid).is_some_and(|value| value.0 == **start))
            .collect();
        if remaining.is_empty() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "owned jobs remain: {remaining:?}"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(identity(leader).is_none(), "leader must be reaped last");
    assert!(fixture.unrelated.try_wait().unwrap().is_none());
    fixture.unrelated.kill().unwrap();
    assert!(fixture.unrelated.wait().unwrap().code().is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn issue270_reset_finish_cleans_multiple_nested_job_groups() {
    let (generation, mut fixture) = owned_jobs().await;
    let leader = generation.pid.as_raw_nonzero().get();
    let mut state = State::new();
    state.generation = Some(generation);
    let (_, mut stop) = tokio::sync::watch::channel(crate::web_sound::Shutdown::Running);
    // The actual reset path calls State::finish before starting its replacement.
    state.finish(crate::Value::Null, &mut stop).await;
    assert_cleaned(leader, &mut fixture).await;
    let replacement =
        Generation::start(&config(), "printf replacement270; exec sleep 600").unwrap();
    assert!(identity(replacement.pid.as_raw_nonzero().get()).is_some());
    drop(replacement);
}

#[tokio::test(flavor = "current_thread")]
async fn issue270_final_owner_drop_cleans_multiple_nested_job_groups() {
    let before = rustix::process::child_subreaper().unwrap();
    let (generation, mut fixture) = owned_jobs().await;
    let leader = generation.pid.as_raw_nonzero().get();
    let mut state = State::new();
    state.generation = Some(generation);
    // Owner final shutdown takes/drops this same generation.
    drop(state);
    assert_cleaned(leader, &mut fixture).await;
    assert_eq!(rustix::process::child_subreaper().unwrap(), before);
}

#[tokio::test(flavor = "current_thread")]
async fn issue270_exited_leader_retained_until_job_groups_are_cleaned() {
    let (mut generation, mut fixture) = owned_jobs().await;
    let leader = generation.pid.as_raw_nonzero().get();
    write(generation.master.clone(), b"exit 23\n")
        .await
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while generation.alive().unwrap() {
        assert!(Instant::now() < deadline, "shell did not exit");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(generation.exit_code().unwrap(), Some(23));
    assert_eq!(identity(leader).unwrap().1, 'Z');
    drop(generation);
    assert_cleaned(leader, &mut fixture).await;
}

#[tokio::test(flavor = "current_thread")]
async fn issue270_shell_exit_produces_eof_with_hup_resistant_jobs() {
    let (mut generation, mut fixture) = owned_jobs().await;
    let leader = generation.pid.as_raw_nonzero().get();
    write(generation.master.clone(), b"exit 23\n")
        .await
        .unwrap();
    // Exercise the owner loop's actual trigger: background slave descriptors
    // must not keep read() blocked forever after the session leader exits.
    tokio::time::timeout(Duration::from_secs(5), async {
        while !read(generation.master.clone()).await.unwrap().is_empty() {}
    })
    .await
    .expect("exited shell's job groups kept the PTY open");
    assert_eq!(generation.exit_code().unwrap(), Some(23));
    assert_eq!(identity(leader).unwrap().1, 'Z');
    drop(generation);
    assert_cleaned(leader, &mut fixture).await;
}

#[tokio::test(flavor = "current_thread")]
async fn issue270_stopped_helper_close_is_bounded_and_cleans_jobs() {
    let (generation, mut fixture) = owned_jobs().await;
    let leader = generation.pid.as_raw_nonzero().get();
    let stat = std::fs::read_to_string(format!("/proc/{leader}/stat")).unwrap();
    let helper: i32 = stat
        .rsplit_once(')')
        .unwrap()
        .1
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    // A baseline without a supervisor must fail without stopping the test runner.
    assert_ne!(helper as u32, std::process::id());
    let completed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let cancel = completed.clone();
    // Bound the RED case too: the old unconditional wait would otherwise hang
    // the entire CI process after its control-read timeout.
    let rescue = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(6);
        while !cancel.load(std::sync::atomic::Ordering::SeqCst) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        if !cancel.load(std::sync::atomic::Ordering::SeqCst) {
            rustix::process::kill_process(Pid::from_raw(helper).unwrap(), Signal::CONT).unwrap();
        }
    });
    rustix::process::kill_process(Pid::from_raw(helper).unwrap(), Signal::STOP).unwrap();
    let began = Instant::now();
    drop(generation);
    let elapsed = began.elapsed();
    completed.store(true, std::sync::atomic::Ordering::SeqCst);
    rescue.join().unwrap();
    assert!(
        elapsed < Duration::from_millis(5500),
        "stopped-helper close took {elapsed:?}"
    );
    assert_cleaned(leader, &mut fixture).await;
    assert!(identity(helper).is_none(), "helper was not reaped");
}

#[tokio::test(flavor = "current_thread")]
async fn issue270_non_utf8_unrelated_comm_does_not_abort_cleanup() {
    let (generation, mut fixture) = owned_jobs().await;
    let leader = generation.pid.as_raw_nonzero().get();
    // Keep the shell itself alive after writing a byte that is invalid UTF-8.
    // Exec would reset comm, so block in the shell's read builtin instead.
    let mut unrelated = Command::new("/bin/sh")
        .args([
            "-c",
            "printf '\\377270) unrelated' > /proc/$$/comm; printf ready; read token",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut ready = [0; 5];
    use std::io::Read;
    unrelated
        .stdout
        .as_mut()
        .unwrap()
        .read_exact(&mut ready)
        .unwrap();
    assert_eq!(&ready, b"ready");
    let stat = std::fs::read(format!("/proc/{}/stat", unrelated.id())).unwrap();
    assert!(std::str::from_utf8(&stat).is_err());
    // Reuse the fixture's original-owner child handle for panic cleanup as well.
    fixture.unrelated.kill().unwrap();
    fixture.unrelated.wait().unwrap();
    fixture.unrelated = unrelated;
    drop(generation);
    assert_cleaned(leader, &mut fixture).await;
}
