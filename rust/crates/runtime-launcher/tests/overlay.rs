use openpilot_runtime_launcher::overlay::{activate, Outcome};
use std::{
    fs,
    time::{Duration, SystemTime},
};

fn checkout() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("openpilot");
    let staging = root.path().join("safe_staging");
    fs::create_dir_all(repo.join(".git/objects")).unwrap();
    fs::create_dir_all(staging.join("finalized")).unwrap();
    fs::write(repo.join("identity"), "original").unwrap();
    fs::write(staging.join("finalized/identity"), "updated").unwrap();
    fs::write(staging.join("finalized/.overlay_consistent"), "").unwrap();
    fs::write(repo.join(".overlay_init"), "").unwrap();
    fs::File::open(repo.join(".overlay_init"))
        .unwrap()
        .set_modified(SystemTime::now() + Duration::from_secs(10))
        .unwrap();
    (root, repo, staging)
}

#[test]
fn activation_keeps_original_backup_and_requires_consistent_unchanged_checkout() {
    let (_root, repo, staging) = checkout();
    assert_eq!(activate(&repo, &staging).unwrap(), Outcome::Activated);
    assert_eq!(
        fs::read_to_string(repo.join("identity")).unwrap(),
        "updated"
    );
    assert_eq!(
        fs::read_to_string(staging.join("old_openpilot/identity")).unwrap(),
        "original"
    );
    assert!(!staging.join("finalized").exists());
}

#[test]
fn repository_edits_incomplete_updates_and_existing_backup_are_preserved() {
    for condition in ["edited", "incomplete", "backup"] {
        let (_root, repo, staging) = checkout();
        match condition {
            "edited" => {
                fs::write(repo.join(".git/objects/new"), "local edit").unwrap();
                fs::File::open(repo.join(".git/objects/new"))
                    .unwrap()
                    .set_modified(SystemTime::now() + Duration::from_secs(20))
                    .unwrap();
            }
            "incomplete" => fs::remove_file(staging.join("finalized/.overlay_consistent")).unwrap(),
            "backup" => fs::create_dir(staging.join("old_openpilot")).unwrap(),
            _ => unreachable!(),
        }
        assert_eq!(activate(&repo, &staging).unwrap(), Outcome::Unchanged);
        assert_eq!(
            fs::read_to_string(repo.join("identity")).unwrap(),
            "original"
        );
        assert_eq!(
            fs::read_to_string(staging.join("finalized/identity")).unwrap(),
            "updated"
        );
    }
}
