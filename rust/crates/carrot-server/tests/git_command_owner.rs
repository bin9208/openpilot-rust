//! Real public Git-status cancellation/timeout with a leader that exits while
//! its owned descendant retains stdout. No PID reuse or unrelated signalling.
use openpilot_carrot_server::git_status::{Repository, Service, State};
use std::{
    fs,
    os::unix::{fs::PermissionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
const FIXTURE: &str = r#"#!/bin/bash
read -r stat < "/proc/$$/stat"
rest=${stat#*) }
set -- $rest
printf '%s\n' "$3" > "$GIT_OWNER_ROOT/group"
/bin/sleep 20 &
printf '%s\n' "$!" > "$GIT_OWNER_ROOT/descendant"
printf 'true\n'
"#;
fn pid(path: &Path) -> Option<u32> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}
fn traces(root: &Path) -> Vec<String> {
    fs::read_dir(root)
        .unwrap()
        .map(Result::unwrap)
        .filter(|e| e.file_name().to_string_lossy().starts_with("trace."))
        .map(|e| fs::read_to_string(e.path()).unwrap())
        .collect()
}
fn reap(line: &str, group: u32) -> bool {
    (line.starts_with(&format!("wait4({group},")) && line.ends_with(&format!("= {group}")))
        || (line.starts_with(&format!("waitid(P_PID, {group},"))
            && line.contains(&format!("si_pid={group}"))
            && !line.contains("WNOWAIT")
            && line.ends_with("= 0"))
}
fn observed(line: &str, group: u32) -> bool {
    reap(line, group)
        || (line.starts_with(&format!("waitid(P_PID, {group},"))
            && line.contains(&format!("si_pid={group}"))
            && line.contains("WNOWAIT")
            && line.ends_with("= 0"))
}
async fn worker(root: &Path, mode: &str) {
    let service = Service::with_clock(
        Repository {
            directory: root.to_owned(),
            lock: root.join("repository.lock"),
            launcher: std::env::current_exe().unwrap(),
        },
        || 1000.,
    );
    let active = service.clone();
    let task = tokio::spawn(async move { active.get(true).await });
    if mode == "cancel" {
        let start = Instant::now();
        loop {
            if let (Some(group), Some(_)) =
                (pid(&root.join("group")), pid(&root.join("descendant")))
            {
                if traces(root)
                    .iter()
                    .any(|trace| trace.lines().any(|l| observed(l.trim(), group)))
                {
                    fs::write(root.join("exit-observed-before-cancel"), group.to_string()).unwrap();
                    break;
                }
            }
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "leader exit was not observed"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        service.wait_idle().await;
    } else {
        let status = task.await.unwrap().unwrap();
        assert!(matches!(status.state, State::Error));
    }
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.join("repository.lock"))
        .unwrap();
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive)
        .expect("completed Git work must release its repository lock");
    fs::write(root.join("worker-result"), mode).unwrap();
}
fn check(mode: &str) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    fs::create_dir(root.join("bin")).unwrap();
    let fixture = root.join("bin/git");
    fs::write(&fixture, FIXTURE).unwrap();
    fs::set_permissions(&fixture, fs::Permissions::from_mode(0o500)).unwrap();
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
            "trace=clone,clone3,fork,vfork,execve,setpgid,setsid,wait4,waitid,kill",
            "-o",
        ])
        .arg(root.join("trace"))
        .arg(std::env::current_exe().unwrap())
        .args(["--worker", mode])
        .env("GIT_OWNER_ROOT", root)
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", root.join("bin").display()),
        )
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .process_group(0)
        .spawn()
        .expect("mandatory owned-process regression requires strace");
    let start = Instant::now();
    let status = loop {
        if let Some(status) = trace.try_wait().unwrap() {
            break status;
        }
        if start.elapsed() >= Duration::from_secs(16) {
            trace.kill().unwrap();
            trace.wait().unwrap();
            panic!(
                "owned Git regression timed out: {}",
                fs::read_to_string(root.join("stderr")).unwrap()
            );
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(
        status.success(),
        "worker failed: {}",
        fs::read_to_string(root.join("stderr")).unwrap()
    );
    assert_eq!(
        fs::read_to_string(root.join("worker-result")).unwrap(),
        mode
    );
    if mode == "cancel" {
        assert!(root.join("exit-observed-before-cancel").exists());
    }
    let group = pid(&root.join("group")).unwrap();
    let records = traces(root);
    let signal_prefix = format!("kill(-{group}, SIGKILL)");
    let owners: Vec<_> = records
        .iter()
        .filter(|t| t.lines().any(|l| l.starts_with(&signal_prefix)))
        .collect();
    assert_eq!(
        owners.len(),
        1,
        "exactly one owner must perform final group cleanup: {records:?}"
    );
    let lines: Vec<_> = owners[0].lines().map(str::trim).collect();
    let final_signal = lines
        .iter()
        .rposition(|l| l.starts_with(&signal_prefix))
        .unwrap();
    assert!(
        !lines[..final_signal].iter().any(|l| reap(l, group)),
        "leader was reaped before final group signal: {}",
        owners[0]
    );
    assert!(
        lines[..final_signal]
            .iter()
            .any(|l| l.contains("WNOWAIT") && observed(l, group)),
        "exit must remain observed but waitable until cleanup: {}",
        owners[0]
    );
    assert_eq!(
        lines[final_signal + 1..]
            .iter()
            .filter(|l| reap(l, group))
            .count(),
        1,
        "leader must be reaped once after all group signals: {}",
        owners[0]
    );
    let descendant = pid(&root.join("descendant")).unwrap();
    if let Ok(stat) = fs::read_to_string(format!("/proc/{descendant}/stat")) {
        let state = stat.rsplit_once(") ").unwrap().1;
        assert!(
            state.starts_with('Z') || state.starts_with('X'),
            "owned descendant still active: {stat}"
        );
    }
    println!("PASS Git owner {mode}, reserved identity through final signal and lock released");
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if let Some(descriptor) = arguments.first().filter(|p| Path::new(p).is_file()) {
        openpilot_process_supervision::run_child(fs::File::open(descriptor)?)?;
        return Ok(());
    }
    if arguments.first().is_some_and(|v| v == "--worker") {
        let root = PathBuf::from(std::env::var_os("GIT_OWNER_ROOT").unwrap());
        let mode = arguments[1].to_str().unwrap();
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(worker(&root, mode));
    } else {
        let mut failures = Vec::new();
        for mode in ["cancel", "timeout"] {
            if std::panic::catch_unwind(|| check(mode)).is_err() {
                failures.push(mode);
            }
        }
        assert!(failures.is_empty(), "Git owner failures: {failures:?}");
    }
    Ok(())
}
