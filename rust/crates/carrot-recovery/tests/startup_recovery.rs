//! Owned repositories and recipients only: no remote network or device reboot.
use openpilot_carrot_recovery::{actions::StartupBackend, config::Config};
use openpilot_hardware_control::reboot;
use openpilot_startup_recovery::state::Update;
use std::{
    fs::{self, File},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};
fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().into()
}
fn script(path: &Path, text: &str) {
    fs::write(path, text).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}
fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|p| Path::new(p).is_file()) {
        openpilot_process_supervision::run_child(File::open(&args[0]).unwrap()).unwrap();
        return;
    }
    if let Some(code) = openpilot_carrot_recovery::actions::internal_command(&args).unwrap() {
        std::process::exit(code);
    }
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo");
    fs::create_dir(&root).unwrap();
    git(&root, &["init", "-q", "-b", "candidate"]);
    git(&root, &["config", "user.name", "Fixture"]);
    git(&root, &["config", "user.email", "fixture@example.invalid"]);
    fs::write(root.join("source"), "old").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "old"]);
    let old = git(&root, &["rev-parse", "HEAD"]);
    fs::write(root.join("source"), "fixed").unwrap();
    git(&root, &["commit", "-qam", "fix"]);
    let target = git(&root, &["rev-parse", "HEAD"]);
    let remote = temp.path().join("remote.git");
    git(
        temp.path(),
        &[
            "clone",
            "--bare",
            root.to_str().unwrap(),
            remote.to_str().unwrap(),
        ],
    );
    git(&root, &["reset", "--hard", &old]);
    let origin = "https://github.com/bin9208/openpilot-rust.git";
    git(&root, &["remote", "add", "origin", origin]);
    git(
        &root,
        &[
            "config",
            &format!("url.file://{}.insteadOf", remote.display()),
            origin,
        ],
    );
    let lock = temp.path().join("repo.lock");
    std::env::set_var("CARROT_REPO_LOCK_PATH", &lock);
    std::env::set_var("GIT_TERMINAL_PROMPT", "0");
    let launcher = std::env::var_os("STARTUP_RECOVERY_TEST_LAUNCHER")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_exe().unwrap());
    let trace = temp.path().join("stage.trace");
    let reboot_trace = temp.path().join("reboot.trace");
    // Never overwrite an installed/runtime helper. The launcher must be copied
    // to this fixture's owned bin directory before constructing Config below.
    let owned_bin = temp.path().join("bin");
    fs::create_dir(&owned_bin).unwrap();
    let owned_launcher = owned_bin.join("openpilot-process-child");
    fs::copy(&launcher, &owned_launcher).unwrap();
    // QEMU exec wrappers must point at an owned ARM copy so current_exe() in
    // the action process locates the same owned process-child sibling.
    if let Some(binary) = std::env::var_os("STARTUP_RECOVERY_TEST_ARM_BINARY") {
        let binary = PathBuf::from(binary);
        let owned_binary = owned_bin.join("fixture-arm");
        fs::copy(&binary, &owned_binary).unwrap();
        let wrapper = fs::read_to_string(&owned_launcher).unwrap();
        assert!(wrapper.contains(binary.to_str().unwrap()));
        fs::write(
            &owned_launcher,
            wrapper.replace(binary.to_str().unwrap(), owned_binary.to_str().unwrap()),
        )
        .unwrap();
    }

    let runtime_bin = root.join("rust/bin");
    fs::create_dir_all(&runtime_bin).unwrap();
    let installer = runtime_bin.join("openpilot-runtime-install");
    let package = |code: i32| {
        script(&installer,&format!("#!/bin/sh\nif flock -n '{}' true; then exit 77; fi\nprintf '%s\\n' \"$@\" > '{}'\nexit {code}\n",lock.display(),trace.display()))
    };
    package(0);
    let system = temp.path().join("system");
    fs::create_dir(&system).unwrap();
    fs::write(system.join("AGNOS"), "").unwrap();
    let sound = temp.path().join("sound");
    script(&sound, "#!/bin/sh\nexit 0\n");
    let sudo = temp.path().join("sudo");
    script(
        &sudo,
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nexit 0\n",
            reboot_trace.display()
        ),
    );
    let config = Config {
        repository: root.clone(),
        web: root.clone(),
        launcher: owned_launcher.clone(),
        data: temp.path().join("data"),
        params: temp.path().join("params"),
        tmux_log: temp.path().join("tmux.log"),
    };
    let backend = |automatic| {
        StartupBackend::new(
            config.clone(),
            reboot::Config {
                system: system.clone(),
                repository: root.clone(),
                helper: sound.clone(),
                launcher: owned_launcher.clone(),
                sudo: sudo.clone(),
                params: temp.path().join("params/d"),
            },
            automatic,
            None,
        )
    };
    let update = Update::default();
    update.run_update(&mut backend(true), true);
    assert_eq!(
        update.snapshot().status,
        "rebooting",
        "{:?}",
        update.snapshot()
    );
    assert_eq!(update.snapshot().detail, &target[..10]);
    let staged = fs::read_to_string(&trace).unwrap();
    assert!(staged.contains(&target) && staged.contains("--ff-only"));
    assert_eq!(fs::read_to_string(&reboot_trace).unwrap(), "-n\nreboot\n");
    assert_eq!(git(&root, &["rev-parse", "HEAD"]), old);
    assert_eq!(fs::read_to_string(root.join("source")).unwrap(), "old");
    fs::remove_file(&trace).unwrap();
    fs::remove_file(&reboot_trace).unwrap();
    git(&root, &["reset", "--hard", &target]);
    let update = Update::default();
    update.run_update(&mut backend(true), true);
    assert_eq!(update.snapshot().status, "waiting");
    assert!(!trace.exists() && !reboot_trace.exists());
    update.run_update(&mut backend(false), false);
    assert_eq!(
        update.snapshot().status,
        "rebooting",
        "{:?}",
        update.snapshot()
    );
    assert!(trace.exists() && reboot_trace.exists());
    fs::remove_file(&trace).unwrap();
    fs::remove_file(&reboot_trace).unwrap();
    fs::write(root.join("source"), "preserve local edit").unwrap();
    let update = Update::default();
    update.run_update(&mut backend(false), false);
    assert_eq!(update.snapshot().status, "failed");
    assert!(update.snapshot().detail.contains("Local changes"));
    assert!(!trace.exists() && !reboot_trace.exists());
    assert_eq!(
        fs::read_to_string(root.join("source")).unwrap(),
        "preserve local edit"
    );
    git(&root, &["restore", "source"]);
    let held = File::open(&lock).unwrap();
    held.lock().unwrap();
    let update = Update::default();
    update.run_update(&mut backend(false), false);
    assert_eq!(update.snapshot().status, "failed");
    assert!(!trace.exists());
    held.unlock().unwrap();
    package(22);
    let update = Update::default();
    update.run_update(&mut backend(false), false);
    assert_eq!(update.snapshot().status, "failed");
    assert!(update.snapshot().detail.contains("exit 22"));
    assert!(!reboot_trace.exists());
    assert_eq!(git(&root, &["rev-parse", "HEAD"]), target);
    package(0);
    let update = Update::default();
    update.run_rebuild(&mut backend(false));
    assert_eq!(update.snapshot().status, "rebuild_rebooting");
    let staged = fs::read_to_string(&trace).unwrap();
    assert!(staged.contains("HEAD") && staged.contains("--branch\ncandidate"));
    assert!(reboot_trace.exists());
    assert_eq!(git(&root, &["rev-parse", "HEAD"]), target);
    fs::remove_file(&trace).unwrap();
    fs::remove_file(&reboot_trace).unwrap();
    let status_file = temp.path().join("progress.json");
    let invoke = |authorized: bool, options: &[&str]| {
        Command::new(&owned_launcher)
            .arg("__startup-action")
            .arg("--repo")
            .arg(&root)
            .arg("--state-file")
            .arg(&status_file)
            .args(options)
            .env(
                "CARROT_STARTUP_RECOVERY",
                if authorized { "1" } else { "0" },
            )
            .env("OPENPILOT_ROOT", &root)
            .output()
            .unwrap()
    };
    assert!(!invoke(false, &[]).status.success());
    assert!(!status_file.exists() && !trace.exists());
    assert!(!invoke(true, &["--pending", "bad"]).status.success());
    assert!(!invoke(true, &["--automatic", "--rebuild"]).status.success());
    assert!(!status_file.exists() && !trace.exists());
    assert!(invoke(true, &["--automatic"]).status.success());
    let status = openpilot_carrot_recovery::actions::startup_action::read(&status_file).unwrap();
    assert_eq!(status.state.status, "waiting", "{status:?}");
    assert!(status.pending_reboot.is_none());
    assert!(!trace.exists());
    // The complete real action reaches its device guard after preparing the
    // fake package. It must report failure, never reboot this workstation.
    let result = invoke(true, &["--automatic", "--pending", &target]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let status = openpilot_carrot_recovery::actions::startup_action::read(&status_file).unwrap();
    assert_eq!(status.state.status, "failed");
    assert_eq!(
        status.state.detail,
        "Device reboot is disabled on this computer."
    );
    assert_eq!(status.pending_reboot, Some(target.clone()));
    assert!(trace.exists());
    assert!(!reboot_trace.exists());
    assert_eq!(git(&root, &["rev-parse", "HEAD"]), target);
    fs::write(&status_file, vec![b'x'; 32769]).unwrap();
    assert!(openpilot_carrot_recovery::actions::startup_action::read(&status_file).is_err());
    // The owner must be a real Linux subreaper. QEMU lacks that prctl, so ARM
    // runs use a native exec supervisor that sets/verifies it before QEMU.
    if std::env::var("STARTUP_RECOVERY_TEST_SUBREAPER").as_deref() != Ok("prepared") {
        openpilot_carrot_recovery::actions::startup_worker::initialize_process().unwrap();
    }
    std::env::set_var("CARROT_STARTUP_RECOVERY", "1");
    let action_helper = owned_bin.join("openpilot-carrot-recovery");
    fs::copy(&owned_launcher, &action_helper).unwrap();
    let mut worker =
        openpilot_carrot_recovery::actions::startup_worker::Worker::new(config.clone());
    assert!(worker.start(true, false).unwrap());
    assert!(!worker.start(false, false).unwrap());
    let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while worker.running() {
        worker.poll().unwrap();
        assert!(std::time::Instant::now() < until, "worker did not finish");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        worker.progress().state.status,
        "waiting",
        "{:?}",
        worker.progress()
    );
    script(&action_helper, "#!/bin/sh\nexit 7\n");
    assert!(worker.start(true, false).unwrap());
    while worker.running() {
        worker.poll().unwrap();
        assert!(std::time::Instant::now() < until);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(worker.progress().state.status, "failed");
    let descendant = temp.path().join("descendant.pid");
    script(
        &action_helper,
        &format!(
            "#!/bin/sh\ntrap '' TERM\nsleep 30 &\necho $! > '{}'\nwait\n",
            descendant.display()
        ),
    );
    assert!(worker.start(true, false).unwrap());
    while !descendant.exists() {
        assert!(std::time::Instant::now() < until);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let pid = fs::read_to_string(&descendant).unwrap().trim().to_owned();
    worker.stop().unwrap();
    assert!(!worker.running());
    assert!(!Path::new(&format!("/proc/{pid}")).exists());
    println!("PASS owned recovery worker: real action wait, duplicate exclusion, unexpected exit, session descendant kill/reap");
    println!("PASS isolated action CLI: admission, invalid pending/action, no-change wait, pending retry, non-device refusal, bounded state");
    println!("PASS 7 owned startup recovery transactions: staging, unchanged automatic wait, manual retry, dirty, lock busy, staging failure, repair; active source preserved");
}
