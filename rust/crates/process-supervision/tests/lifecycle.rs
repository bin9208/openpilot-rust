use openpilot_logging::producer::Factory;
use openpilot_process_supervision::{
    Execution, ManagedProcess, NativeCommand, ProcessLog, StopOptions,
};
use std::{ffi::OsString, path::PathBuf, time::Duration};

fn process(cwd: PathBuf, argv: Vec<OsString>) -> ManagedProcess {
    let logger = Factory::new("inproc://process-supervision-unit".into())
        .unwrap()
        .logger();
    ManagedProcess::new(
        "fixture".into(),
        Execution::Native(NativeCommand {
            launcher: PathBuf::from(env!("CARGO_BIN_EXE_openpilot-process-child")),
            basedir: PathBuf::from("/"),
            cwd,
            argv,
        }),
        ProcessLog::new(logger),
    )
}

#[test]
fn child_execution_failure_keeps_a_real_handle_until_stop() {
    let directory = tempfile::tempdir().unwrap();
    let mut child = process(directory.path().into(), vec!["./missing".into()]);
    child.start().unwrap();
    let pid = child.state().unwrap().pid;
    assert!(pid > 0);
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while child.exit_code().unwrap().is_none() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(child.exit_code().unwrap(), Some(1));
    child.start().unwrap();
    assert_eq!(child.state().unwrap().pid, pid);
    assert_eq!(child.stop(StopOptions::default()).unwrap(), Some(1));
    let state = child.state().unwrap();
    assert_eq!(
        (
            state.running,
            state.should_be_running,
            state.pid,
            state.exit_code
        ),
        (false, false, 0, 0)
    );
}

#[test]
fn invalid_cwd_fails_in_child_instead_of_parent() {
    let directory = tempfile::tempdir().unwrap();
    let mut child = process(directory.path().join("missing"), vec!["/bin/true".into()]);
    child.start().unwrap();
    assert!(child.state().unwrap().pid > 0);
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while child.exit_code().unwrap().is_none() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(child.stop(StopOptions::default()).unwrap(), Some(1));
}
