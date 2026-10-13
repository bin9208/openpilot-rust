use openpilot_web_upload::WebDav;
use std::{fs, os::unix::fs::PermissionsExt};

fn config(path: &std::path::Path, url: &str) {
    fs::write(
        path,
        serde_json::json!({"url":url,"username":"fixture-user","password":"fixture-secret"})
            .to_string(),
    )
    .unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}

#[test]
fn private_config_is_optional_and_https_only() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("private_upload.json");
    assert!(WebDav::load(&path).unwrap().is_none());
    config(&path, "https://example.test:5128/openpilot/");
    assert!(WebDav::load(&path).unwrap().is_some());
    for url in [
        "http://example.test/openpilot/",
        "https://user:secret@example.test/",
        "https://example.test/?secret=1",
        "https://example.test/#secret",
        "https://example.test/openpilot/\n",
    ] {
        config(&path, url);
        let error = WebDav::load(&path).err().unwrap();
        let text = format!("{error} {error:?}");
        assert!(!text.contains("example.test"));
        assert!(!text.contains("fixture-secret"));
        assert!(!text.contains("user:secret"));
    }
}

#[test]
fn private_config_rejects_unprotected_files_and_symlinks() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("private_upload.json");
    config(&path, "https://example.test/openpilot/");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    assert!(WebDav::load(&path).is_err());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let link = root.path().join("link.json");
    std::os::unix::fs::symlink(&path, &link).unwrap();
    assert!(WebDav::load(&link).is_err());
    assert!(WebDav::load(root.path()).is_err());
    fs::write(&path, "fixture-secret broken JSON").unwrap();
    let error = WebDav::load(&path).err().unwrap();
    assert!(!format!("{error} {error:?}").contains("fixture-secret"));
}
