//! Native counterpart of test_updated_process_probe.py. Read failures are injected
//! at the filesystem boundary to remain meaningful under privileged CI users.
use openpilot_dev_tools::ci_policy::process_alive_from;
use std::io;
#[test]
fn exited_process_during_open_or_read_is_not_alive() {
    for errno in [2, 3] {
        assert!(!process_alive_from(|| Err(io::Error::from_raw_os_error(errno))).unwrap());
    }
}
#[test]
fn zombie_is_stopped_and_running_worker_is_alive() {
    for (state, expected) in [('Z', false), ('S', true)] {
        assert_eq!(
            process_alive_from(|| Ok(format!("123 (worker) {state} 1 2 3"))).unwrap(),
            expected
        );
    }
}
#[test]
fn permission_failure_is_not_an_exit() {
    let error = process_alive_from(|| Err(io::Error::from_raw_os_error(13))).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
}
