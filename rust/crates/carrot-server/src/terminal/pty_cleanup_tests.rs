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

async fn owned_jobs() -> (Generation, Fixture) {
    let subreaper = rustix::process::child_subreaper().unwrap();
    let generation = Generation::start(&config(), "exec /bin/bash --noprofile --norc -i").unwrap();
    assert_eq!(rustix::process::child_subreaper().unwrap(), subreaper);
    let mut fixture = Fixture {
        members: BTreeMap::new(),
        unrelated: Command::new("/bin/sleep").arg("600").spawn().unwrap(),
    };
    write(
        generation.master.clone(),
        br#"set -m; trap '' HUP; /bin/bash -c 'trap "" HUP; sleep 600 & printf "NESTED270=%s\n" "$!"; wait' & printf 'JOB1270=%s\n' "$!"; sleep 600 & printf 'JOB2270=%s\n' "$!"
"#,
    )
    .await
    .unwrap();
    let mut output = String::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    while fixture.members.len() < 3 {
        let bytes = tokio::time::timeout_at(deadline.into(), read(generation.master.clone()))
            .await
            .expect("fixture PTY readiness deadline")
            .unwrap();
        assert!(!bytes.is_empty(), "shell exited during setup: {output}");
        output.push_str(&String::from_utf8_lossy(&bytes));
        for line in output.lines() {
            let line = line.trim_end_matches('\r');
            if let Some(pid) = ["NESTED270=", "JOB1270=", "JOB2270="]
                .into_iter()
                .find_map(|prefix| line.strip_prefix(prefix))
                .and_then(|pid| pid.parse::<i32>().ok())
            {
                fixture.members.insert(pid, identity(pid).unwrap().0);
            }
        }
    }
    let leader = generation.pid.as_raw_nonzero().get();
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
    assert!(
        groups.len() >= 2,
        "fixture needs multiple job-control groups"
    );
    println!("issue270 owned session={leader} groups={groups:?} output={output:?}");
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
