//! Exercise the public metadata operation through private executable fixtures.
//! All process environments are per-child; the test runner never changes PATH.
use rustix::process::{kill_process_group, waitid, Pid, Signal, WaitId, WaitIdOptions};
use std::{
    fs,
    io::Write,
    os::unix::{fs::symlink, process::CommandExt},
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Owner(Child);
impl Drop for Owner {
    fn drop(&mut self) {
        // The test does not reap before its final group cleanup.
        let pid = Pid::from_raw(self.0.id().try_into().unwrap()).unwrap();
        let _ = kill_process_group(pid, Signal::KILL);
        let _ = self.0.wait();
    }
}

#[expect(
    clippy::zombie_processes,
    reason = "the regression requires leader exit before pipe-holder exit; the outer owner cleans the group"
)]
fn fixture(mode: &str) {
    match mode {
        "large-stdout" => std::io::stdout().write_all(&vec![b'x'; 262_144]).unwrap(),
        "large-stderr" => {
            std::io::stderr().write_all(&vec![b'e'; 262_144]).unwrap();
            println!("metadata-ok");
        }
        "held-pipe" => {
            // The direct child exits, but a bounded private descendant holds
            // both pipes. The caller must still observe its total deadline.
            let _child = Command::new(std::env::current_exe().unwrap())
                .arg("--hold")
                .spawn()
                .unwrap();
            println!("incomplete-pipes");
        }
        "invalid-stdout" => std::io::stdout().write_all(b"\xff").unwrap(),
        "invalid-stderr" => {
            std::io::stderr().write_all(b"\xff").unwrap();
            println!("unusable");
        }
        "failure" => std::process::exit(7),
        "empty" => println!(" \t "),
        _ => panic!("unknown fixture {mode}"),
    }
}

fn worker(root: &Path, mode: &str) {
    let start = Instant::now();
    let actual =
        openpilot_dashcam_upload::metadata::git_text(root, &["--fixture", mode], "fallback");
    let expected = match mode {
        "large-stdout" => "x".repeat(262_144),
        "large-stderr" => "metadata-ok".into(),
        _ => "fallback".into(),
    };
    assert_eq!(
        actual.len(),
        expected.len(),
        "wrong metadata length for {mode}"
    );
    assert_eq!(actual, expected, "wrong metadata for {mode}");
    if mode == "held-pipe" {
        assert!(start.elapsed() >= Duration::from_secs(4));
    }
    assert!(
        start.elapsed() < Duration::from_secs(6),
        "total four-second deadline escaped: {mode}"
    );
}

fn check(mode: &str) {
    let root = tempfile::tempdir().unwrap();
    symlink(std::env::current_exe().unwrap(), root.path().join("git")).unwrap();
    let stderr = root.path().join("stderr");
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--worker", mode])
        .arg(root.path())
        .env("PATH", root.path())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(fs::File::create(&stderr).unwrap())
        .process_group(0);
    let owned = Owner(command.spawn().unwrap());
    let pid = Pid::from_raw(owned.0.id().try_into().unwrap()).unwrap();
    let end = Instant::now() + Duration::from_secs(7);
    let status = loop {
        match waitid(
            WaitId::Pid(pid),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        ) {
            Ok(Some(status)) => break status,
            Ok(None) => (),
            Err(rustix::io::Errno::INTR) => continue,
            Err(error) => panic!("observe private worker: {error}"),
        }
        assert!(
            Instant::now() < end,
            "metadata worker escaped deadline: {mode}"
        );
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(
        status.exit_status(),
        Some(0),
        "{mode}: {}",
        fs::read_to_string(stderr).unwrap()
    );
    drop(owned);
    println!("PASS git capture {mode}");
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("--fixture") => fixture(&args[2]),
        Some("--hold") => std::thread::sleep(Duration::from_secs(6)),
        Some("--worker") => worker(Path::new(&args[3]), &args[2]),
        _ => {
            let mut failures = Vec::new();
            for mode in [
                "large-stdout",
                "large-stderr",
                "held-pipe",
                "invalid-stdout",
                "invalid-stderr",
                "failure",
                "empty",
            ] {
                if std::panic::catch_unwind(|| check(mode)).is_err() {
                    failures.push(mode);
                }
            }
            assert!(failures.is_empty(), "git capture failures: {failures:?}");
        }
    }
}
