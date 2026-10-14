use openpilot_process_supervision::CapturedCommand;
use std::{
    fs::{self, File},
    os::{fd::AsFd, unix::ffi::OsStringExt},
};

fn main() {
    if let Some(descriptor) = std::env::args_os()
        .nth(1)
        .filter(|path| std::path::Path::new(path).is_file())
    {
        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(descriptor).unwrap()).unwrap();
        let cwd: Vec<u8> = serde_json::from_value(value["cwd"].clone()).unwrap();
        let cwd = std::path::PathBuf::from(std::ffi::OsString::from_vec(cwd));
        rustix::process::setsid().unwrap();
        fs::write(cwd.join("pid"), std::process::id().to_string()).unwrap();
        std::process::exit(23);
    }
    locked_session_pre_handshake_exit_closes_and_reaps();
}

fn locked_session_pre_handshake_exit_closes_and_reaps() {
    let root = tempfile::tempdir().unwrap();
    let helper = std::env::current_exe().unwrap();
    let record = root.path().join("pid");
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
