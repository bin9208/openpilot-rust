use super::wait_for_exec;
use rustix::process::{waitid, Pid, WaitId, WaitIdOptions};
use std::{os::unix::net::UnixListener, process::Command};

#[test]
fn failed_handshake_keeps_leader_unreaped_for_group_cleanup() {
    let root = tempfile::tempdir().unwrap();
    let listener = UnixListener::bind(root.path().join("exec.sock")).unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut child = Command::new("/bin/sh")
        .args(["-c", "exit 23"])
        .spawn()
        .unwrap();
    let error = wait_for_exec(&listener, &mut child, None).unwrap_err();
    assert!(error
        .to_string()
        .contains("helper exited before exec handshake"));
    let pid = Pid::from_raw(i32::try_from(child.id()).unwrap()).unwrap();
    let observed = waitid(
        WaitId::Pid(pid),
        WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
    )
    .unwrap()
    .unwrap();
    assert_eq!(observed.exit_status(), Some(23));
    assert_eq!(child.wait().unwrap().code(), Some(23));
    println!(
        "{}",
        serde_json::json!({"pid": child.id(), "error": error.to_string(), "unreaped_exit_status": observed.exit_status(), "final_wait_exit": 23})
    );
}
