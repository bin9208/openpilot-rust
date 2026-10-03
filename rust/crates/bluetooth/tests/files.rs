use openpilot_bluetooth::atomic_json;
use serde::{Serialize, Serializer};
use std::{fs, os::unix::fs::PermissionsExt};

struct InvalidValue;

impl Serialize for InvalidValue {
    fn serialize<S: Serializer>(&self, _serializer: S) -> Result<S::Ok, S::Error> {
        Err(serde::ser::Error::custom("intentional encoding failure"))
    }
}

#[test]
fn original_file_survives_when_json_encoding_fails() {
    let root = tempfile::tempdir().expect("test directory");
    let path = root.path().join("runtime/status.json");
    atomic_json(&path, &serde_json::json!({"original": true})).expect("initial file");
    let original = fs::read(&path).expect("read initial file");
    assert!(atomic_json(&path, &InvalidValue).is_err());
    assert_eq!(fs::read(&path).expect("retained file"), original);
    assert_eq!(
        fs::read_dir(path.parent().expect("parent"))
            .expect("runtime entries")
            .count(),
        1
    );
}

#[test]
fn replacement_is_private_when_target_was_world_readable() {
    let root = tempfile::tempdir().expect("test directory");
    let path = root.path().join("status.json");
    fs::write(&path, b"old").expect("old file");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("old mode");
    atomic_json(&path, &serde_json::json!({"new": true})).expect("atomic replacement");
    assert_eq!(
        fs::metadata(&path)
            .expect("new metadata")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&fs::read(&path).expect("new bytes"))
            .expect("complete JSON"),
        serde_json::json!({"new": true})
    );
}
