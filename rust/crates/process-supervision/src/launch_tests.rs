use super::wait_for_exec;
use rustix::process::{waitid, Pid, WaitId, WaitIdOptions};
use std::{
    io::Write,
    os::unix::net::{UnixListener, UnixStream},
    process::Command,
};

#[test]
fn queued_exec_report_survives_helper_exit_without_reaping() {
    for report in [
        2_i32.to_ne_bytes().to_vec(),
        13_i32.to_ne_bytes().to_vec(),
        vec![],
        vec![2],
    ] {
        let root = tempfile::tempdir().unwrap();
        let socket = root.path().join("exec.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();
        let mut child = Command::new("/bin/sh")
            .args(["-c", "exit 23"])
            .spawn()
            .unwrap();
        let pid = Pid::from_raw(i32::try_from(child.id()).unwrap()).unwrap();
        waitid(
            WaitId::Pid(pid),
            WaitIdOptions::EXITED | WaitIdOptions::NOWAIT,
        )
        .unwrap();
        let mut stream = UnixStream::connect(&socket).unwrap();
        stream.write_all(&report).unwrap();
        drop(stream);
        let result = wait_for_exec(&listener, &mut child, None);
        match report.as_slice() {
            [] => result.unwrap(),
            [a, b, c, d] => match result.unwrap_err() {
                crate::Error::Io(error) => assert_eq!(
                    error.raw_os_error(),
                    Some(i32::from_ne_bytes([*a, *b, *c, *d]))
                ),
                error => panic!("expected queued errno, got {error}"),
            },
            _ => assert!(result
                .unwrap_err()
                .to_string()
                .contains("invalid exec handshake")),
        }
        assert_eq!(
            waitid(
                WaitId::Pid(pid),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT
            )
            .unwrap()
            .unwrap()
            .exit_status(),
            Some(23)
        );
        assert_eq!(child.wait().unwrap().code(), Some(23));
    }
}

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
