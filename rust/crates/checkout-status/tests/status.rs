use openpilot_checkout_status::{read_checkout_commit, UpdateStatus};
use std::{fs, path::Path};

fn metadata(repo: &Path, commit: &str) {
    fs::write(
        repo.join("build.json"),
        format!(r#"{{"openpilot":{{"git_commit":"{commit}"}}}}"#),
    )
    .unwrap();
}

#[test]
fn changed_checkout_needs_two_due_reads() {
    let repo = tempfile::tempdir().unwrap();
    let original = "a".repeat(40);
    metadata(repo.path(), &original);
    let mut status = UpdateStatus::new(repo.path(), "/unused/native-helper");
    metadata(repo.path(), &"b".repeat(40));

    let states = [status.update(0.0), status.update(4.999), status.update(5.0)];

    assert_eq!(states, [false, false, true]);
    assert_eq!(status.running_commit(), Some(original.as_str()));
}

#[test]
fn missing_startup_identity_is_never_recaptured() {
    let repo = tempfile::tempdir().unwrap();
    let mut status = UpdateStatus::new(repo.path(), "/unused/native-helper");
    metadata(repo.path(), &"a".repeat(40));

    let states = [status.update(0.0), status.update(5.0)];

    assert_eq!(states, [false, false]);
    assert_eq!(status.running_commit(), None);
}

#[test]
fn unrelated_python_json_values_do_not_reject_a_valid_commit() {
    let repo = tempfile::tempdir().unwrap();
    let commit = "A".repeat(64);
    fs::write(
        repo.path().join("build.json"),
        format!(r#"{{"other":[NaN,Infinity,-Infinity,"\ud800"],"openpilot":{{"git_commit":"{commit}"}}}}"#),
    )
    .unwrap();

    let actual = read_checkout_commit(repo.path(), Path::new("/unused/native-helper"));

    assert_eq!(actual, Some(commit.to_ascii_lowercase()));
}
