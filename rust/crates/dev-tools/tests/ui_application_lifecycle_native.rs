//! Real owned process and ZMQ endpoints; no user display or runtime namespace.
#![cfg(feature = "ui-application-lifecycle-native")]
use openpilot_dev_tools::{
    qa_process::CommandSpec,
    ui_application_lifecycle_check::{Backend, NativeBackend},
};
use std::{path::Path, time::Duration};
#[test]
fn actual_native_log_packet_poll_and_close() {
    let work = tempfile::tempdir().unwrap();
    let mut backend = NativeBackend::new(Path::new("unused"), Path::new("unused"));
    let address = format!("ipc://{}", work.path().join("application-logs").display());
    backend.bind_logs(&address).unwrap();
    let context = zmq::Context::new();
    let sender = context.socket(zmq::PUSH).unwrap();
    sender.connect(&address).unwrap();
    let packet=b"\x14{\"levelnum\":20,\"msg\":\"raylib: owned native packet\",\"pathname\":\"/source/logging.rs\"}";
    sender.send(packet.as_slice(), 0).unwrap();
    assert_eq!(backend.log_packet().unwrap().unwrap(), packet);
    assert!(backend.log_packet().unwrap().is_none());
    drop(sender);
    drop(context);
    backend.close_logs().unwrap();
}
#[test]
fn actual_live_child_timeout_kill_reap_and_no_reused_pid_signals() {
    let work = tempfile::tempdir().unwrap();
    let mut backend = NativeBackend::new(Path::new("unused"), Path::new("unused"));
    let command = CommandSpec::new(vec!["/bin/sleep".into(), "30".into()], Duration::MAX);
    let pid = backend
        .start_live(&command, &work.path().join("child.log"))
        .unwrap();
    assert!(backend.poll_live().unwrap().is_none());
    assert!(backend
        .wait_live(Some(Duration::from_millis(1)))
        .unwrap()
        .is_none());
    let (exe, maps) = backend.identity(pid).unwrap();
    assert_eq!(exe, std::fs::canonicalize("/bin/sleep").unwrap());
    assert!(!maps.to_lowercase().contains("libpython"));
    backend.kill_live().unwrap();
    assert_eq!(backend.wait_live(None).unwrap(), Some(-9));
    assert_eq!(backend.poll_live().unwrap(), Some(-9));
    backend.kill_live().unwrap();
    assert!(!Path::new(&format!("/proc/{pid}")).exists());
    let command = CommandSpec::new(vec!["/bin/true".into()], Duration::MAX);
    let pid = backend
        .start_live(&command, &work.path().join("child2.log"))
        .unwrap();
    assert_eq!(backend.wait_live(None).unwrap(), Some(0));
    backend.kill_live().unwrap();
    assert_eq!(backend.poll_live().unwrap(), Some(0));
    assert!(!Path::new(&format!("/proc/{pid}")).exists());
}
#[test]
fn dropping_backend_kills_and_reaps_only_its_owned_child() {
    let work = tempfile::tempdir().unwrap();
    let pid;
    {
        let mut backend = NativeBackend::new(Path::new("unused"), Path::new("unused"));
        pid = backend
            .start_live(
                &CommandSpec::new(vec!["/bin/sleep".into(), "30".into()], Duration::MAX),
                &work.path().join("drop.log"),
            )
            .unwrap();
    }
    assert!(!Path::new(&format!("/proc/{pid}")).exists());
}
