use openpilot_process_supervision::CapturedCommand;
use std::{
    fs::{self, File},
    os::{fd::AsFd, unix::fs::PermissionsExt},
};

#[test]
fn locked_session_pre_handshake_exit_closes_and_reaps() {
    let root = tempfile::tempdir().unwrap();
    let helper = root.path().join("owned-helper");
    let record = root.path().join("pid");
    fs::write(&helper, format!("#!/usr/bin/python3\nimport os\nfrom pathlib import Path\nos.setsid()\nPath({:?}).write_text(str(os.getpid()))\nraise SystemExit(23)\n", record.to_string_lossy())).unwrap();
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
    let lock = File::create(root.path().join("lock")).unwrap();
    lock.lock().unwrap();
    let group = rustix::process::getpgrp();
    let request = CapturedCommand {
        launcher: helper,
        cwd: root.path().into(),
        argv: vec!["/bin/true".into()],
    };
    let error = match request.spawn_session_merged_with_lock(&[], lock.as_fd()) {
        Ok(_) => panic!("helper exit unexpectedly accepted"),
        Err(error) => error,
    };
    let pid: u32 = fs::read_to_string(&record).unwrap().parse().unwrap();
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    assert_eq!(rustix::process::getpgrp(), group);
    drop(lock);
    File::options()
        .read(true)
        .write(true)
        .open(root.path().join("lock"))
        .unwrap()
        .try_lock()
        .unwrap();
    println!(
        "{}",
        serde_json::json!({"pid":pid,"error":error.to_string(),"final_reaped":true,"parent_group_preserved":true,"lock_released":true})
    );
}
