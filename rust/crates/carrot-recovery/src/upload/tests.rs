use super::*;
use std::{fs, os::unix::fs::PermissionsExt};

#[test]
fn private_config_failure_stops_recovery_primary_upload_without_api_fallback() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("state")).unwrap();
    let path = root.path().join("state/private_upload.json");
    fs::write(&path, "fixture-secret malformed JSON").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let config = Config {
        repository: root.path().to_owned(),
        web: root.path().to_owned(),
        launcher: root.path().join("launcher"),
        data: root.path().to_owned(),
        params: root.path().join("params"),
        tmux_log: root.path().join("tmux.log"),
    };
    let error = dsm(&config, &[], b"fixture").unwrap_err();
    assert!(error.to_string().contains("private upload configuration"));
    assert!(!error.to_string().contains("fixture-secret"));
}
