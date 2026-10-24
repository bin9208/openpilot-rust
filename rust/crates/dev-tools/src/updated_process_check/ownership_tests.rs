//! Real syscall ownership tests, separate from the frozen policy baseline.
use super::*;
use std::os::unix::fs::PermissionsExt;
fn number(path: &Path) -> Option<u32> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}
fn tid() -> u32 {
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
fn probe_owner_worker() {
    let Some(root) = std::env::var_os("UPDATED_PROBE_OWNER_ROOT").map(PathBuf::from) else {
        return;
    };
    let mode = std::env::var("UPDATED_PROBE_OWNER_MODE").unwrap();
    fs::write(root.join("owner"), tid().to_string()).unwrap();
    if matches!(mode.as_str(), "reported-worker" | "active-worker") {
        let mut child = Command::new(root.join("helper"))
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let (_, reported) = worker_ready(&root.join("worker"), rustix::process::getpid()).unwrap();
        if mode == "active-worker" {
            drop(reported);
            assert_eq!(child.wait().unwrap().signal(), Some(9));
        } else {
            fs::write(root.join("release"), b"go").unwrap();
            assert!(child.wait().unwrap().success());
            // A guard can outlive its worker. No later numeric signal is safe.
            drop(reported);
        }
    } else {
        let probe = Probe::spawn(
            &root.join("helper"),
            &json!({"owned":"synthetic-probe"}),
            &BTreeMap::new(),
        )
        .unwrap();
        let result = probe.capture(Duration::from_secs(4));
        match mode.as_str() {
            "success" => {
                let value = result.unwrap();
                assert_eq!(value.code, 0);
                assert_eq!(value.stdout, "owned\n");
                assert_eq!(value.stderr, "");
            }
            "nonzero" => {
                let value = result.unwrap();
                assert_eq!(value.code, 7);
                assert_eq!(value.stdout, "owned failure\n");
                assert_eq!(value.stderr, "");
            }
            "invalid" => assert!(result
                .err()
                .expect("invalid UTF8 must fail")
                .to_string()
                .contains("invalid utf-8")),
            "timeout" => assert!(result
                .err()
                .expect("live child must time out")
                .to_string()
                .contains("probe timed out")),
            _ => panic!("unknown probe owner fixture"),
        }
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
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let body=match mode {
        "success"=>"/bin/cat >/dev/null; printf 'owned\\r\\n'",
        "nonzero"=>"/bin/cat >/dev/null; printf 'owned failure\\r'; exit 7",
        "invalid"=>"/bin/cat >/dev/null; printf '\\377'",
        "timeout"=>"/bin/cat >/dev/null; /bin/sleep 20",
        "reported-worker"=>"printf '%s' $$ > \"$UPDATED_PROBE_OWNER_ROOT/worker.group\"; printf '%s' $$ > \"$UPDATED_PROBE_OWNER_ROOT/worker\"; while [ ! -e \"$UPDATED_PROBE_OWNER_ROOT/release\" ]; do /bin/sleep .005; done",
        "active-worker"=>"printf '%s' $$ > \"$UPDATED_PROBE_OWNER_ROOT/worker.group\"; printf '%s' $$ > \"$UPDATED_PROBE_OWNER_ROOT/worker\"; while [ ! -e \"$UPDATED_PROBE_OWNER_ROOT/release\" ]; do :; done",
        _=>unreachable!(),
    };
    let script =
        format!("#!/bin/bash\nprintf '%s' $$ > \"$UPDATED_PROBE_OWNER_ROOT/group\"\n{body}\n");
    fs::write(root.join("helper"), script).unwrap();
    fs::set_permissions(root.join("helper"), fs::Permissions::from_mode(0o500)).unwrap();
    let mut trace=Command::new("/usr/bin/strace")
        .args(["-ff","-qq","--kill-on-exit","-s","256","-e","trace=clone,clone3,fork,vfork,execve,setpgid,wait4,waitid,kill,pidfd_open,pidfd_send_signal","-o"])
        .arg(root.join("trace")).arg(std::env::current_exe().unwrap())
        .args(["--exact","updated_process_check::ownership_tests::probe_owner_worker","--nocapture"])
        .env("UPDATED_PROBE_OWNER_ROOT",root).env("UPDATED_PROBE_OWNER_MODE",mode)
        .stdin(Stdio::null()).stdout(fs::File::create(root.join("stdout")).unwrap()).stderr(fs::File::create(root.join("stderr")).unwrap())
        .process_group(0).spawn().expect("ownership regression requires actual strace");
    let began = Instant::now();
    let status = loop {
        if let Some(status) = trace.try_wait().unwrap() {
            break status;
        }
        if began.elapsed() >= Duration::from_secs(10) {
            trace.kill().unwrap();
            trace.wait().unwrap();
            panic!("owned checker probe timed out");
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
        "reported group must belong to the real direct child: {trace}"
    );
    let signals: Vec<_> = rows
        .iter()
        .enumerate()
        .filter(|(_, s)| s.starts_with(&format!("kill(-{group}, SIGKILL)")))
        .collect();
    if matches!(mode, "reported-worker" | "active-worker") {
        assert!(signals.is_empty(),"reported identity may be released before guard drop; numeric group signal is forbidden: {trace}");
        assert!(
            rows.iter()
                .any(|s| s.starts_with(&format!("pidfd_open({group},"))),
            "reported worker must be pinned while live: {trace}"
        );
        assert!(
            !rows
                .iter()
                .any(|s| s.starts_with(&format!("kill({group},"))),
            "reported worker must never use a raw numeric pid signal: {trace}"
        );
        if mode == "active-worker" {
            assert!(
                rows.iter()
                    .any(|s| s.starts_with("pidfd_send_signal(") && s.contains("SIGKILL")),
                "active cleanup must exercise the pinned worker signal: {trace}"
            );
        }
    } else {
        assert_eq!(
            signals.len(),
            1,
            "one final cleanup signal per owned probe: {trace}"
        );
        let signal = signals[0].0;
        assert!(
            !rows[..signal].iter().any(|s| reaped(s, group)),
            "checker probe reaped before numeric group cleanup: {trace}"
        );
        assert_eq!(
            rows[signal + 1..]
                .iter()
                .filter(|s| reaped(s, group))
                .count(),
            1,
            "one reap after owned group cleanup: {trace}"
        );
        if mode != "timeout" {
            assert!(
                rows[..signal]
                    .iter()
                    .any(|s| s.starts_with(&format!("waitid(P_PID, {group},"))
                        && s.contains("WNOWAIT")
                        && s.contains(&format!("si_pid={group}"))),
                "completed probe stays waitable through cleanup: {trace}"
            );
        }
    }
}
#[test]
fn successful_probe_keeps_identity_until_group_cleanup() {
    check("success");
}
#[test]
fn nonzero_probe_keeps_identity_until_group_cleanup() {
    check("nonzero");
}
#[test]
fn malformed_probe_output_keeps_identity_through_decode_error() {
    check("invalid");
}
#[test]
fn probe_timeout_still_terminates_owned_descendants() {
    check("timeout");
}
#[test]
fn reported_worker_guard_never_signals_released_numeric_group() {
    check("reported-worker");
}

struct FixtureChild(Child);
impl Drop for FixtureChild {
    fn drop(&mut self) {
        if self.0.try_wait().unwrap().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}
fn held_identity(root: &Path, name: &str) -> FixtureChild {
    let marker = root.join(name);
    let child = Command::new("/bin/bash")
        .args([
            "-c",
            "printf '%s' $$ > \"$1\"; exec /bin/sleep 20",
            "owned-identity",
        ])
        .arg(&marker)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .unwrap();
    let child = FixtureChild(child);
    let began = Instant::now();
    while number(&marker).is_none() {
        assert!(
            began.elapsed() < Duration::from_secs(2),
            "identity fixture failed to start"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(number(&marker), Some(child.0.id()));
    child
}
#[test]
fn reported_worker_rejects_wrong_parent_without_signalling_any_fixture() {
    let root = tempfile::tempdir().unwrap();
    let child = held_identity(root.path(), "worker");
    let group = Pid::from_child(&child.0);
    let wrong_parent = rustix::process::getppid().unwrap();
    assert!(pin_worker(i32::try_from(child.0.id()).unwrap(), group, wrong_parent).is_err());
    assert!(alive(i32::try_from(child.0.id()).unwrap()).unwrap());
}
#[test]
fn reported_worker_rejects_other_group_without_signalling_either_fixture() {
    let root = tempfile::tempdir().unwrap();
    let leader = held_identity(root.path(), "leader");
    let other = held_identity(root.path(), "other");
    assert!(pin_worker(
        i32::try_from(other.0.id()).unwrap(),
        Pid::from_child(&leader.0),
        rustix::process::getpid()
    )
    .is_err());
    assert!(alive(i32::try_from(leader.0.id()).unwrap()).unwrap());
    assert!(alive(i32::try_from(other.0.id()).unwrap()).unwrap());
}

#[test]
fn active_reported_worker_is_terminated_through_its_verified_pidfd() {
    check("active-worker");
}
