//! Source Popen(start_new_session=True) requires a session as well as a group.
#![cfg(feature = "lagd-tools-native")]
use std::process::Command;
#[test]
fn source_child_new_session_preserves_parent_session_and_all_exec_arguments() {
    let parent = rustix::process::getsid(None).unwrap();
    let mut command = Command::new("/bin/sh");
    command.args(["-c",r#"read pid name state ppid pgrp session rest < /proc/$$/stat; printf '%s %s %s\n' "$pid" "$pgrp" "$session""#]);
    openpilot_process_limits::new_session(&mut command);
    let output = command.output().unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let values = std::str::from_utf8(&output.stdout)
        .unwrap()
        .split_whitespace()
        .map(|v| v.parse::<u32>().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(values.len(), 3);
    assert_eq!(values[0], values[1]);
    assert_eq!(values[0], values[2]);
    assert_ne!(values[0], parent.as_raw_nonzero().get() as u32);
    assert_eq!(rustix::process::getsid(None).unwrap(), parent);
}
