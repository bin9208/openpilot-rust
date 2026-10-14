use openpilot_logging::producer::Factory;
use openpilot_process_supervision::{
    ensure_running, Execution, ManagedProcess, NativeCommand, ProcessLog,
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

#[test]
fn wanted_model_daemons_restart_after_exit_without_a_special_crash_policy() {
    for name in ["modeld", "dmonitoringmodeld"] {
        let directory = tempfile::tempdir().unwrap();
        let logger = Factory::new(format!("inproc://camera-restart-{name}"))
            .unwrap()
            .logger();
        let process = ManagedProcess::new(
            name.into(),
            Execution::Native(NativeCommand {
                launcher: env!("CARGO_BIN_EXE_openpilot-process-child").into(),
                basedir: PathBuf::from("/"),
                cwd: directory.path().into(),
                argv: vec!["/bin/false".into()],
            }),
            ProcessLog::new(logger),
        );
        assert!(!process.policy.restart_if_crash);
        let mut processes = [process];
        ensure_running(&mut processes, &[], |_| Ok(true)).unwrap();
        let first = processes[0].state().unwrap().pid;
        assert!(first > 0);
        let deadline = Instant::now() + Duration::from_secs(3);
        while processes[0].exit_code().unwrap().is_none() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(processes[0].exit_code().unwrap(), Some(1));
        ensure_running(&mut processes, &[], |_| Ok(true)).unwrap();
        let second = processes[0].state().unwrap().pid;
        assert!(second > 0);
        assert_ne!(
            second, first,
            "wanted daemon must acquire fresh process state"
        );
        let deadline = Instant::now() + Duration::from_secs(3);
        while processes[0].exit_code().unwrap().is_none() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(processes[0].exit_code().unwrap(), Some(1));
        ensure_running(&mut processes, &[], |_| Ok(false)).unwrap();
        assert!(
            !processes[0].has_process(),
            "offroad/disabled predicate must prevent restart"
        );
    }
}
