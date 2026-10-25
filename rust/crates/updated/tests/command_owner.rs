//! Linux owner-order regressions. The parent enables this required-feature suite
//! with a freshly built process-child launcher; no borrowed cache or test oracle.
use openpilot_updated::{
    process::{Commands, NativeCommands},
    signals::Wake,
    Error,
};
use std::{
    fs,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};
fn number(path: &Path) -> Option<u32> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}
fn worker_tid() -> u32 {
    fs::read_link("/proc/thread-self")
        .unwrap()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .parse()
        .unwrap()
}
#[test]
fn command_owner_worker() {
    let Some(root) = std::env::var_os("UPDATED_OWNER_ROOT").map(PathBuf::from) else {
        return;
    };
    let mode = std::env::var("UPDATED_OWNER_MODE").unwrap();
    fs::write(root.join("owner"), worker_tid().to_string()).unwrap();
    let launcher =
        std::env::var_os("UPDATED_OWNER_LAUNCHER").expect("mandatory fresh process-child launcher");
    let wake = Arc::new(Wake::default());
    let controller = (mode == "held").then(|| {
        let root = root.clone();
        let wake = wake.clone();
        std::thread::spawn(move || {
            let began = Instant::now();
            loop {
                if let (Some(group), Some(_)) = (
                    number(&root.join("group")),
                    number(&root.join("descendant")),
                ) {
                    // In this control, a descendant retains stdout after leader
                    // exit; cancellation must also work while capture is draining.
                    if fs::read_to_string(format!("/proc/{group}/stat"))
                        .ok()
                        .is_some_and(|s| s.rsplit_once(") ").unwrap().1.starts_with('Z'))
                    {
                        wake.stop();
                        return;
                    }
                }
                assert!(
                    began.elapsed() < Duration::from_secs(4),
                    "held stdout leader did not exit"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
        })
    });
    let body = match mode.as_str() {
        "success" => "printf '%s\\n' $$ > \"$1/group\"; printf 'owned\\r\\n'",
        "nonzero" => "printf '%s\\n' $$ > \"$1/group\"; printf 'owned failure\\r'; exit 7",
        "invalid" => "printf '%s\\n' $$ > \"$1/group\"; printf '\\377'",
        "held" => "printf '%s\\n' $$ > \"$1/group\"; /bin/sleep 20 & printf '%s\\n' $! > \"$1/descendant\"; exit 0",
        _ => panic!("unknown owner fixture"),
    };
    let argv = [
        "/bin/bash",
        "-c",
        body,
        "owned-updater-command",
        root.to_str().unwrap(),
    ]
    .map(str::to_owned);
    let mut commands = NativeCommands {
        launcher: launcher.into(),
        wake,
    };
    let result = commands.run(&argv, Some(&root));
    if let Some(controller) = controller {
        controller.join().unwrap();
    }
    match mode.as_str() {
        "success" => assert_eq!(result.unwrap(), "owned\n"),
        "nonzero" => match result {
            Err(Error::Command { code, output, .. }) => {
                assert_eq!(code, 7);
                assert_eq!(output, "owned failure\n");
            }
            other => panic!("{other:?}"),
        },
        "invalid" => assert!(matches!(result, Err(Error::Utf8(_))), "{result:?}"),
        "held" => assert!(matches!(result, Err(Error::Interrupted)), "{result:?}"),
        _ => unreachable!(),
    }
    fs::write(root.join("result"), mode).unwrap();
}
fn reaped(line: &str, pid: u32) -> bool {
    (line.starts_with(&format!("wait4({pid},")) && line.ends_with(&format!("= {pid}")))
        || (line.starts_with(&format!("waitid(P_PID, {pid},"))
            && line.contains(&format!("si_pid={pid}"))
            && !line.contains("WNOWAIT")
            && line.ends_with("= 0"))
}
fn check(mode: &str) {
    let launcher =
        std::env::var_os("UPDATED_OWNER_LAUNCHER").expect("mandatory fresh process-child launcher");
    assert!(Path::new(&launcher).is_file());
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let mut traced = Command::new("/usr/bin/strace")
        .args([
            "-ff",
            "-qq",
            "--kill-on-exit",
            "-s",
            "256",
            "-e",
            "trace=clone,clone3,fork,vfork,execve,setsid,wait4,waitid,kill",
            "-o",
        ])
        .arg(root.join("trace"))
        .arg(std::env::current_exe().unwrap())
        .args(["--exact", "command_owner_worker", "--nocapture"])
        .env("UPDATED_OWNER_ROOT", root)
        .env("UPDATED_OWNER_MODE", mode)
        .env("UPDATED_OWNER_LAUNCHER", launcher)
        .stdin(Stdio::null())
        .stdout(fs::File::create(root.join("stdout")).unwrap())
        .stderr(fs::File::create(root.join("stderr")).unwrap())
        .process_group(0)
        .spawn()
        .expect("ownership regression requires actual strace");
    let began = Instant::now();
    let status = loop {
        if let Some(status) = traced.try_wait().unwrap() {
            break status;
        }
        if began.elapsed() >= Duration::from_secs(10) {
            traced.kill().unwrap();
            traced.wait().unwrap();
            panic!("owned updater trace timed out");
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(
        status.success(),
        "{}\n{}",
        fs::read_to_string(root.join("stdout")).unwrap(),
        fs::read_to_string(root.join("stderr")).unwrap()
    );
    assert_eq!(fs::read_to_string(root.join("result")).unwrap(), mode);
    let owner = number(&root.join("owner")).unwrap();
    let group = number(&root.join("group")).unwrap();
    let trace = fs::read_to_string(root.join(format!("trace.{owner}"))).unwrap();
    let rows: Vec<_> = trace.lines().map(str::trim).collect();
    assert!(
        rows.iter().any(|s| (s.starts_with("clone(")
            || s.starts_with("clone3(")
            || s.starts_with("vfork("))
            && s.ends_with(&format!("= {group}"))),
        "reported group must be direct owned command: {trace}"
    );
    let signals: Vec<_> = rows
        .iter()
        .enumerate()
        .filter(|(_, s)| s.starts_with(&format!("kill(-{group}, SIGKILL)")))
        .collect();
    assert_eq!(
        signals.len(),
        1,
        "one cleanup signal, no destructor signal after reaping: {trace}"
    );
    let signal = signals[0].0;
    assert!(
        !rows[..signal].iter().any(|s| reaped(s, group)),
        "released updater identity before numeric group signal: {trace}"
    );
    assert_eq!(
        rows[signal + 1..]
            .iter()
            .filter(|s| reaped(s, group))
            .count(),
        1,
        "exactly one reap after group cleanup: {trace}"
    );
    if mode != "held" {
        assert!(
            rows[..signal]
                .iter()
                .any(|s| s.starts_with(&format!("waitid(P_PID, {group},"))
                    && s.contains("WNOWAIT")
                    && s.contains(&format!("si_pid={group}"))),
            "exited command must remain waitable: {trace}"
        );
    } else if let Some(child) = number(&root.join("descendant")) {
        if let Ok(stat) = fs::read_to_string(format!("/proc/{child}/stat")) {
            let state = stat.rsplit_once(") ").unwrap().1;
            assert!(
                state.starts_with('Z') || state.starts_with('X'),
                "held descendant active: {stat}"
            );
        }
    }
}
#[test]
fn success_has_no_group_signal_after_reaping_or_on_drop() {
    check("success");
}
#[test]
fn nonzero_has_no_group_signal_after_reaping_or_on_drop() {
    check("nonzero");
}
#[test]
fn invalid_utf8_retains_identity_through_error_cleanup() {
    check("invalid");
}
#[test]
fn interrupt_cleans_descendant_holding_stdout_after_leader_exit() {
    check("held");
}
