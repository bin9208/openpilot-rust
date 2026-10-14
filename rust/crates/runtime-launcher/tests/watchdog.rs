use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

struct Owned(Child);
impl Drop for Owned {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn start(root: &Path, repo: &Path) -> Owned {
    Owned(
        Command::new(env!("CARGO_BIN_EXE_openpilot-carrot-web-watchdog"))
            .arg(repo)
            .env("CARROT_WEB_PID_FILE", root.join("pid"))
            .env("CARROT_WEB_LOCK_FILE", root.join("lock"))
            .env("CARROT_WEB_PORT", "17263")
            .env("CARROT_WEB_RESTART_DELAY", "0.02")
            .env("OWNED_RECORD", root.join("record"))
            .stdout(Stdio::null())
            .spawn()
            .unwrap(),
    )
}

fn wait(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(Instant::now() < deadline, "watchdog operation timed out");
        thread::sleep(Duration::from_millis(10));
    }
}

fn provider(repo: &Path, generation: &str) {
    let bin = repo.join("rust/bin");
    fs::create_dir_all(&bin).unwrap();
    let path = bin.join("openpilot-carrot-server");
    fs::write(&path, format!("#!/bin/sh\nprintf '{generation} %s %s %s\\n' \"$$\" \"$CARROT_WEB_EXTERNAL\" \"$*\" >> \"$OWNED_RECORD\"\nexit 7\n")).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn retries_current_checkout_and_preserves_pidfile_ownership() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    provider(&repo, "old");
    let mut owner = start(root.path(), &repo);
    wait(|| fs::read_to_string(root.path().join("record")).is_ok_and(|s| s.contains("old ")));
    let mut duplicate = start(root.path(), &repo);
    wait(|| duplicate.0.try_wait().unwrap().is_some());
    assert!(duplicate.0.wait().unwrap().success());
    assert_eq!(
        fs::read_to_string(root.path().join("pid")).unwrap().trim(),
        owner.0.id().to_string()
    );
    fs::rename(&repo, root.path().join("old")).unwrap();
    provider(&repo, "new");
    wait(|| {
        fs::read_to_string(root.path().join("record"))
            .unwrap()
            .contains("new ")
    });
    let record = fs::read_to_string(root.path().join("record")).unwrap();
    assert!(record
        .lines()
        .all(|s| s.ends_with("1 --host 0.0.0.0 --port 17263")));
    fs::write(root.path().join("pid"), "replacement owner\n").unwrap();
    rustix::process::kill_process(
        rustix::process::Pid::from_raw(owner.0.id().try_into().unwrap()).unwrap(),
        rustix::process::Signal::TERM,
    )
    .unwrap();
    wait(|| owner.0.try_wait().unwrap().is_some());
    assert!(owner.0.wait().unwrap().success());
    assert_eq!(
        fs::read_to_string(root.path().join("pid")).unwrap(),
        "replacement owner\n"
    );
    assert!(fs::File::open(root.path().join("lock"))
        .unwrap()
        .try_lock()
        .is_ok());
}

#[test]
fn stale_pid_file_does_not_treat_an_unrelated_live_process_as_owner() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    provider(&repo, "recovered");
    let mut unrelated = Owned(Command::new("sleep").arg("30").spawn().unwrap());
    let pid = format!("{}\n", unrelated.0.id());
    fs::write(root.path().join("pid"), &pid).unwrap();
    let mut child = start(root.path(), &repo);
    wait(|| root.path().join("record").exists() || child.0.try_wait().unwrap().is_some());
    assert!(
        root.path().join("record").exists(),
        "stale PID prevented web server startup"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("pid")).unwrap().trim(),
        child.0.id().to_string()
    );
    assert!(unrelated.0.try_wait().unwrap().is_none());
    rustix::process::kill_process(
        rustix::process::Pid::from_raw(child.0.id().try_into().unwrap()).unwrap(),
        rustix::process::Signal::TERM,
    )
    .unwrap();
    wait(|| child.0.try_wait().unwrap().is_some());
    assert!(child.0.wait().unwrap().success());
    assert!(!root.path().join("pid").exists());
    assert!(unrelated.0.try_wait().unwrap().is_none());
}

#[test]
fn termination_reaps_resistant_server_and_releases_owner_files() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    provider(&repo, "running");
    fs::write(
        repo.join("rust/bin/openpilot-carrot-server"),
        "#!/bin/sh\ntrap '' TERM\nprintf '%s' \"$$\" > \"$OWNED_RECORD\"\nexec sleep 30\n",
    )
    .unwrap();
    let mut owner = start(root.path(), &repo);
    wait(|| fs::read_to_string(root.path().join("record")).is_ok_and(|s| !s.is_empty()));
    let child = fs::read_to_string(root.path().join("record")).unwrap();
    assert!(Path::new("/proc").join(&child).exists());
    rustix::process::kill_process(
        rustix::process::Pid::from_raw(owner.0.id().try_into().unwrap()).unwrap(),
        rustix::process::Signal::TERM,
    )
    .unwrap();
    wait(|| owner.0.try_wait().unwrap().is_some());
    assert!(owner.0.wait().unwrap().success());
    assert!(!Path::new("/proc").join(child).exists());
    assert!(!root.path().join("pid").exists());
    assert!(fs::File::open(root.path().join("lock"))
        .unwrap()
        .try_lock()
        .is_ok());
}

#[test]
fn missing_checkout_retries_until_the_current_tree_appears() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    let mut owner = start(root.path(), &repo);
    wait(|| root.path().join("pid").exists());
    assert!(owner.0.try_wait().unwrap().is_none());
    provider(&repo, "recovered");
    wait(|| root.path().join("record").exists());
    rustix::process::kill_process(
        rustix::process::Pid::from_raw(owner.0.id().try_into().unwrap()).unwrap(),
        rustix::process::Signal::INT,
    )
    .unwrap();
    wait(|| owner.0.try_wait().unwrap().is_some());
    assert!(owner.0.wait().unwrap().success());
}
