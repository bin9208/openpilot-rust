//! Real Git repositories and worktrees; these run in Actions on both host/guest.
use openpilot_runtime_build::source_store::{retain, Snapshot};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const GROWTH: u64 = 8 * 1024 * 1024;

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("/usr/bin/git")
        .args([
            "-c",
            "user.name=Source fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "protocol.file.allow=always",
        ])
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim_end().into()
}

fn commit(root: &Path) {
    git(root, &["add", "."]);
    git(root, &["commit", "--quiet", "-m", "source fixture"]);
}

fn repository(parent: &Path, name: &str) -> PathBuf {
    let root = parent.join(name);
    fs::create_dir(&root).unwrap();
    git(&root, &["init", "--quiet"]);
    fs::write(root.join("launch.sh"), "printf 'original source\\n'\n").unwrap();
    commit(&root);
    root
}

#[test]
fn prior_source_survives_failed_update_and_removal_of_mutable_git_checkout() {
    let temp = tempfile::tempdir().unwrap();
    let source = repository(temp.path(), "mutable");
    let store = temp.path().join("retained");
    let previous = retain(&source, &store, GROWTH).unwrap();
    let reused = retain(&source, &store, 0).unwrap();
    assert_eq!(reused.root, previous.root);
    fs::write(source.join("launch.sh"), "printf 'updated source\\n'\n").unwrap();
    assert!(retain(&source, &store, GROWTH).is_err());
    previous.verify(&store).unwrap();
    commit(&source);
    let updated = retain(&source, &store, GROWTH).unwrap();
    assert_ne!(updated.root, previous.root);
    assert_ne!(updated.source, previous.source);
    fs::remove_dir_all(&source).unwrap();
    for (snapshot, expected) in [
        (&previous, b"original source\n".as_slice()),
        (&updated, b"updated source\n".as_slice()),
    ] {
        snapshot.verify(&store).unwrap();
        let result = Command::new("/bin/sh")
            .arg("launch.sh")
            .current_dir(&snapshot.root)
            .output()
            .unwrap();
        assert!(result.status.success());
        assert_eq!(result.stdout, expected);
        assert_eq!(
            git(&snapshot.root, &["rev-parse", "HEAD"]),
            snapshot.source.commit
        );
    }
}

#[test]
fn initialized_nested_submodules_have_independent_retained_git_directories() {
    let temp = tempfile::tempdir().unwrap();
    let leaf = repository(temp.path(), "leaf");
    let module = repository(temp.path(), "module");
    git(
        &module,
        &[
            "submodule",
            "add",
            "--quiet",
            leaf.to_str().unwrap(),
            "nested",
        ],
    );
    commit(&module);
    let source = repository(temp.path(), "mutable");
    git(
        &source,
        &[
            "submodule",
            "add",
            "--quiet",
            module.to_str().unwrap(),
            "vendor/provider",
        ],
    );
    git(&source, &["submodule", "update", "--init", "--recursive"]);
    commit(&source);
    let store = temp.path().join("retained");
    let retained = retain(&source, &store, GROWTH).unwrap();
    assert_eq!(retained.modules.len(), 2);
    git(&source, &["submodule", "deinit", "-f", "--all"]);
    assert!(retain(&source, &store, GROWTH).is_err());
    for input in [&source, &module, &leaf] {
        fs::remove_dir_all(input).unwrap();
    }
    retained.verify(&store).unwrap();
    assert_eq!(
        fs::read(retained.root.join("vendor/provider/nested/launch.sh")).unwrap(),
        b"printf 'original source\\n'\n"
    );
    fs::write(
        retained.root.join("vendor/provider/nested/launch.sh"),
        "tampered",
    )
    .unwrap();
    assert!(retained.verify(&store).is_err());
}

#[test]
fn interrupted_attempt_and_capacity_or_lock_failure_preserve_ready_source() {
    let temp = tempfile::tempdir().unwrap();
    let source = repository(temp.path(), "mutable");
    let store = temp.path().join("retained");
    let previous = retain(&source, &store, GROWTH).unwrap();
    let interrupted = store.join("checkouts/source-interrupted");
    fs::create_dir(&interrupted).unwrap();
    fs::write(interrupted.join("partial"), "not ready").unwrap();
    fs::write(source.join("README"), "new source").unwrap();
    commit(&source);
    assert!(retain(&source, &store, u64::MAX).is_err());
    let lock = fs::OpenOptions::new()
        .write(true)
        .open(store.join("source.lock"))
        .unwrap();
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::LockExclusive).unwrap();
    assert!(retain(&source, &store, GROWTH).is_err());
    drop(lock);
    let updated = retain(&source, &store, GROWTH).unwrap();
    previous.verify(&store).unwrap();
    updated.verify(&store).unwrap();
    assert_eq!(fs::read(interrupted.join("partial")).unwrap(), b"not ready");
    assert!(previous.root.exists());
}

#[test]
fn real_cli_rejects_corrupt_receipt_or_redirected_store_without_changing_previous_source() {
    let temp = tempfile::tempdir().unwrap();
    let source = repository(temp.path(), "mutable");
    let store = temp.path().join("retained");
    let command = || {
        Command::new(env!("CARGO_BIN_EXE_openpilot-runtime-source"))
            .arg("--root")
            .arg(&source)
            .arg("--store")
            .arg(&store)
            .arg("--expected-growth-bytes")
            .arg(GROWTH.to_string())
            .output()
            .unwrap()
    };
    let result = command();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let retained: Snapshot = serde_json::from_slice(&result.stdout).unwrap();
    let receipt = store
        .join("receipts")
        .join(format!("{}.json", retained.source.commit));
    fs::write(&receipt, b"{truncated").unwrap();
    assert!(!command().status.success());
    retained.verify(&store).unwrap();
    fs::remove_file(&receipt).unwrap();
    std::os::unix::fs::symlink(source.join("launch.sh"), &receipt).unwrap();
    assert!(!command().status.success());
    retained.verify(&store).unwrap();
    let alias = temp.path().join("store-alias");
    std::os::unix::fs::symlink(&store, &alias).unwrap();
    assert!(retain(&source, &alias, GROWTH).is_err());
    assert!(retain(&source, &source.join("cache"), GROWTH).is_err());
}
