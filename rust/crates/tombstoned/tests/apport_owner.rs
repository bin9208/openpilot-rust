//! Actual retrace child ownership ordering. No PID-reuse stress or unrelated
//! process signalling: strace follows only this test's exclusively owned child.
use openpilot_tombstoned::{apport::Retrace, Error};
use std::{
    fs,
    os::unix::{fs::PermissionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

const FIXTURE: &str = r#"#!/bin/bash
read -r stat < "/proc/$$/stat"
rest=${stat#*) }
set -- $rest
printf '%s\n' "$3" > "$TOMBSTONE_OWNER_FIXTURE/group"
if [ "$TOMBSTONE_OWNER_MODE" = held ]; then
  /bin/sleep 20 &
  printf '%s\n' "$!" > "$TOMBSTONE_OWNER_FIXTURE/descendant"
else
  printf '\377'
fi
"#;
fn pid(path: &Path) -> Option<u32> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}
fn exited_observation(line: &str, group: u32) -> bool {
    // The old path reaps with wait4. The corrected path must observe exit with
    // WNOWAIT. Accept either for the cancellation barrier, so RED cannot pass by
    // setting the stop flag before the old try_wait has actually run.
    (line.starts_with(&format!("wait4({group},")) && line.ends_with(&format!("= {group}")))
        || (line.starts_with(&format!("waitid(P_PID, {group},"))
            && line.contains(&format!("si_pid={group}"))
            && line.contains("WNOWAIT")
            && line.ends_with("= 0"))
}
fn trace_path(root: &Path, worker: u32) -> PathBuf {
    root.join(format!("trace.{worker}"))
}
/// Reexecuted under strace by the two real regressions, never an oracle process.
#[test]
fn retrace_owner_worker() {
    let Some(root) = std::env::var_os("TOMBSTONE_OWNER_FIXTURE").map(PathBuf::from) else {
        return;
    };
    let mode = std::env::var("TOMBSTONE_OWNER_MODE").unwrap();
    // libtest runs this worker in a thread; strace -ff names its wait/kill log
    // by that thread ID, not the thread-group/process ID.
    let worker: u32 = fs::read_link("/proc/thread-self")
        .unwrap()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    fs::write(root.join("worker"), worker.to_string()).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let controller = (mode == "held").then(|| {
        let root = root.clone();
        let stop = stop.clone();
        std::thread::spawn(move || {
            let began = Instant::now();
            loop {
                if let (Some(group), Some(_)) =
                    (pid(&root.join("group")), pid(&root.join("descendant")))
                {
                    let trace = fs::read_to_string(trace_path(&root, worker)).unwrap_or_default();
                    if trace
                        .lines()
                        .any(|line| exited_observation(line.trim(), group))
                    {
                        fs::write(root.join("exit-observed-before-cancel"), trace).unwrap();
                        stop.store(true, Ordering::Relaxed);
                        return;
                    }
                }
                assert!(
                    began.elapsed() < Duration::from_secs(4),
                    "no completed owned-child exit observation before cancellation"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
        })
    });
    let retrace = Retrace {
        stop: (mode == "held").then_some(stop),
        ..Retrace::default()
    };
    assert_eq!(retrace.timeout, Duration::from_secs(30));
    let result = retrace.stacktrace(&root.join("input.crash"));
    if let Some(controller) = controller {
        controller.join().unwrap();
    }
    if mode == "held" {
        assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
    } else {
        assert!(matches!(result, Err(Error::Utf8(_))), "{result:?}");
    }
    assert_eq!(
        fs::read(root.join("input.crash")).unwrap(),
        b"owned crash fixture\n"
    );
    fs::write(root.join("worker-result"), mode).unwrap();
}
fn completed_reap(line: &str, group: u32) -> bool {
    (line.starts_with(&format!("wait4({group},")) && line.ends_with(&format!("= {group}")))
        || (line.starts_with(&format!("waitid(P_PID, {group},"))
            && line.contains(&format!("si_pid={group}"))
            && !line.contains("WNOWAIT")
            && line.ends_with("= 0"))
}
fn check_owner(mode: &str) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    fs::create_dir(root.join("bin")).unwrap();
    let fixture = root.join("bin/apport-retrace");
    // Close the only writable descriptor before any fork/exec starts.
    fs::write(&fixture, FIXTURE).unwrap();
    fs::set_permissions(&fixture, fs::Permissions::from_mode(0o500)).unwrap();
    fs::write(root.join("input.crash"), b"owned crash fixture\n").unwrap();
    let stdout = fs::File::create(root.join("stdout")).unwrap();
    let stderr = fs::File::create(root.join("stderr")).unwrap();
    let mut trace = Command::new("/usr/bin/strace")
        .args([
            "-ff",
            "-qq",
            "--kill-on-exit",
            "-s",
            "256",
            "-e",
            "trace=clone,clone3,fork,vfork,execve,setpgid,wait4,waitid,kill",
            "-o",
        ])
        .arg(root.join("trace"))
        .arg(std::env::current_exe().unwrap())
        .args(["--exact", "retrace_owner_worker", "--nocapture"])
        .env("TOMBSTONE_OWNER_FIXTURE", root)
        .env("TOMBSTONE_OWNER_MODE", mode)
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", root.join("bin").display()),
        )
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .process_group(0)
        .spawn()
        .expect("actual ownership regression requires strace");
    let began = Instant::now();
    let status = loop {
        if let Some(status) = trace.try_wait().unwrap() {
            break status;
        }
        if began.elapsed() >= Duration::from_secs(10) {
            // strace --kill-on-exit kills every owned tracee, including retrace's
            // separate process group, rather than leaving held-stdout children.
            trace.kill().unwrap();
            trace.wait().unwrap();
            panic!(
                "owned retrace regression timed out: {}",
                fs::read_to_string(root.join("stderr")).unwrap()
            );
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(
        status.success(),
        "worker failed: {}\n{}",
        fs::read_to_string(root.join("stdout")).unwrap(),
        fs::read_to_string(root.join("stderr")).unwrap()
    );
    assert_eq!(
        fs::read_to_string(root.join("worker-result")).unwrap(),
        mode
    );
    let worker = pid(&root.join("worker")).unwrap();
    let group = pid(&root.join("group")).unwrap();
    let trace = fs::read_to_string(trace_path(root, worker)).unwrap();
    let lines: Vec<_> = trace.lines().map(str::trim).collect();
    assert!(
        lines.iter().any(|l| (l.starts_with("clone(")
            || l.starts_with("clone3(")
            || l.starts_with("fork(")
            || l.starts_with("vfork("))
            && l.ends_with(&format!("= {group}"))),
        "group leader must be this worker's actual direct child: {trace}"
    );
    let signal = lines
        .iter()
        .rposition(|line| line.starts_with(&format!("kill(-{group}, SIGKILL)")))
        .expect("error cleanup must terminate the owned process group");
    assert!(
        !lines[..signal]
            .iter()
            .any(|line| completed_reap(line, group)),
        "group identity released before final numeric group signal: {trace}"
    );
    assert!(
        lines[..signal]
            .iter()
            .any(|line| line.starts_with("waitid(") && exited_observation(line, group)),
        "exited child must remain observed but waitable through cleanup: {trace}"
    );
    assert_eq!(
        lines[signal + 1..]
            .iter()
            .filter(|line| completed_reap(line, group))
            .count(),
        1,
        "direct child must be reaped exactly once after cleanup: {trace}"
    );
    if mode == "held" {
        assert!(root.join("exit-observed-before-cancel").is_file());
        let descendant = pid(&root.join("descendant")).unwrap();
        if let Ok(stat) = fs::read_to_string(format!("/proc/{descendant}/stat")) {
            let after_name = stat.rsplit_once(") ").unwrap().1;
            assert!(
                after_name.starts_with('Z') || after_name.starts_with('X'),
                "owned held-stdout descendant is still active: {stat}"
            );
        }
    }
}
#[test]
fn invalid_utf8_cleanup_retains_waitable_group_identity_until_last_signal() {
    check_owner("invalid");
}
#[test]
fn cancellation_after_observed_exit_cleans_held_stdout_before_reaping_leader() {
    check_owner("held");
}
