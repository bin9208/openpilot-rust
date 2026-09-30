use openpilot_tombstoned::discovery::{clear_apport_folder, get_tombstones};
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
};

#[test]
fn applies_exact_mode_and_follows_symlinks_when_discovering() {
    // Given regular files with differing mode bits, a symlink and a tombstone directory.
    let directory = tempfile::tempdir().unwrap();
    for (name, mode) in [
        ("valid.crash", 0o640),
        ("wrong.crash", 0o600),
        ("special.crash", 0o4640),
    ] {
        let path = directory.path().join(name);
        fs::write(&path, b"data").unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    }
    symlink("valid.crash", directory.path().join("link.crash")).unwrap();
    fs::create_dir(directory.path().join("tombstone-directory")).unwrap();
    // When scanning the original directory boundary.
    let files = get_tombstones(directory.path()).unwrap();
    // Then exact permission/type bits matter only to the .crash suffix branch.
    let mut names = files
        .into_iter()
        .map(|entry| entry.path.file_name().unwrap().to_owned())
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(names, ["link.crash", "tombstone-directory", "valid.crash"]);
}
#[test]
fn startup_cleanup_keeps_hidden_entries_and_ignores_directories() {
    // Given a hidden crash, visible file and visible nonempty directory.
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join(".hidden.crash"), b"hidden").unwrap();
    fs::write(directory.path().join("old.crash"), b"old").unwrap();
    fs::create_dir(directory.path().join("child")).unwrap();
    fs::write(directory.path().join("child/file"), b"child").unwrap();
    // When performing source startup cleanup.
    clear_apport_folder(directory.path());
    // Then glob('*') semantics preserve hidden names and removal failures do not abort.
    assert!(!directory.path().join("old.crash").exists());
    assert!(directory.path().join(".hidden.crash").is_file());
    assert!(directory.path().join("child/file").is_file());
}
#[test]
fn a_dangling_eligible_entry_propagates_stat_failure() {
    // Given an eligible symlink whose target raced away.
    let directory = tempfile::tempdir().unwrap();
    symlink("absent", directory.path().join("tombstone-missing")).unwrap();
    // When scanning.
    let result = get_tombstones(directory.path());
    // Then the whole scan fails as the source stat call does.
    assert!(
        matches!(result,Err(openpilot_tombstoned::Error::Io(error)) if error.kind()==std::io::ErrorKind::NotFound)
    );
}
