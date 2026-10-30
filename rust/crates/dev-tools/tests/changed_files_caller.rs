//! Maintained CI caller regression after accepted codec/lifecycle gates.
#![cfg(all(target_os = "linux", feature = "changed-files-native"))]
use std::{fs, path::Path, process::Command};

fn git(root: &Path, args: &[&str]) -> String {
    let result = Command::new("git")
        .current_dir(root)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap()
}
fn commit(root: &Path, message: &str) {
    git(root, &["add", "."]);
    git(root, &["commit", "-m", message]);
}
fn repository() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    git(root.path(), &["init"]);
    git(root.path(), &["config", "user.name", "CI test"]);
    git(root.path(), &["config", "user.email", "ci@example.invalid"]);
    fs::write(root.path().join("base.txt"), b"base\n").unwrap();
    commit(root.path(), "base");
    git(
        root.path(),
        &["update-ref", "refs/remotes/origin/dev", "HEAD"],
    );
    root
}
fn check(root: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_check-changed-files"))
        .current_dir(root)
        .env("BASE_SHA", "0".repeat(40))
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .unwrap()
}
#[test]
fn initial_push_checks_earlier_commits() {
    // Preserve the original sole unittest's multi-commit initial-push assertion.
    let root = repository();
    fs::write(root.path().join("broken.py"), b"def broken(:\n").unwrap();
    commit(root.path(), "earlier syntax error");
    fs::write(root.path().join("last.txt"), b"last\n").unwrap();
    commit(root.path(), "later unrelated change");
    let output = check(root.path());
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Syntax"));
    assert!(output.stdout.is_empty());
}
#[test]
fn changed_current_worktree_bytes_are_checked_after_head_selection() {
    let root = repository();
    fs::write(root.path().join("changed.py"), b"x=1\n").unwrap();
    commit(root.path(), "valid HEAD source");
    assert!(check(root.path()).status.success());
    fs::write(root.path().join("changed.py"), b"def broken(:\n").unwrap();
    let output = check(root.path());
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Syntax"));
}
#[test]
fn whitespace_precedes_syntax_rejection() {
    let root = repository();
    fs::write(root.path().join("broken.py"), b"def broken(:\n").unwrap();
    fs::write(root.path().join("space.txt"), b"trailing \n").unwrap();
    commit(root.path(), "whitespace and syntax errors");
    let output = check(root.path());
    assert!(!output.status.success());
    let diagnostic = String::from_utf8_lossy(&output.stderr);
    assert!(diagnostic.contains("Whitespace"));
    assert!(!diagnostic.contains("Syntax"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("trailing whitespace"));
}
