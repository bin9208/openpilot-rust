//! Closed four-helper retirement backed by independently audited native proof.
//! Runtime deleter/uploader/xattr/config remain current source dependencies.
use super::{
    corpus::{bytes, hash},
    require, Result,
};
use std::{
    fs,
    path::{Component, Path},
};
pub const ORIGINAL_RUN: u64 = 39791032084;
pub const ORIGINAL_SOURCE: &str = "5fc4e8886dde7d41840758dbdf113143b6bc0a78";
pub const ORIGINAL_ARTIFACT: u64 = 12432035228;
pub const ORIGINAL_ARTIFACT_SHA256: &str =
    "4befab6556f7ad324862b731fc631e028da8b94e61553c96d09afe9b03c7e4e2";
// Accepted complete native proof; arbitrary missing source is never admitted.
pub const ACCEPTED_NATIVE_RUN: u64 = 39802773197;
pub const ACCEPTED_NATIVE_SOURCE: &str = "6acad91831dc215e04f78ca7760da63baedec9a7";
pub const ACCEPTED_NATIVE_ARTIFACT: u64 = 12437510258;
pub const ACCEPTED_NATIVE_ARTIFACT_SHA256: &str =
    "a723124881ec4081847a2865dded20b0e327ec4b17b6b6c2738f5ddafc7f4399";
pub const RETIRED: &[(&str, &str)] = &[
    (
        "rust/tools/check_deleter_reference.py",
        "60b2ca4e6d4e7260bb275f1aacad0108b029fecf1d7b4ed836dcfa5ed72f0a19",
    ),
    (
        "rust/tools/check_deleter_daemon.py",
        "ec9b90f63f80d53db05543cdd56556435030f5e84bf1032cad1022771dcc6ec5",
    ),
    (
        "rust/tools/deleter_fixtures.py",
        "73610dca2ad77b7fe95a3364b7adf86ed7d49069ed991f916dc6dd92022559fd",
    ),
    (
        "rust/tools/tests/test_deleter_observer.py",
        "32078173a58f11e81af9927fae8fb33c1f3d533a45b61f7d025ca773081ba87d",
    ),
];
fn accepted() -> Result<()> {
    let hex = |s: &str, n| s.len() == n && s.bytes().all(|b| b.is_ascii_hexdigit());
    require(
        ACCEPTED_NATIVE_RUN > 0
            && ACCEPTED_NATIVE_ARTIFACT > 0
            && hex(ACCEPTED_NATIVE_SOURCE, 40)
            && hex(ACCEPTED_NATIVE_ARTIFACT_SHA256, 64),
        "deleter original helper retirement has no accepted native proof",
    )
}
pub fn proof() -> serde_json::Value {
    serde_json::json!({"original_run":ORIGINAL_RUN,"original_source":ORIGINAL_SOURCE,"original_artifact":ORIGINAL_ARTIFACT,"original_artifact_sha256":ORIGINAL_ARTIFACT_SHA256,"native_run":ACCEPTED_NATIVE_RUN,"native_source":ACCEPTED_NATIVE_SOURCE,"native_artifact":ACCEPTED_NATIVE_ARTIFACT,"native_artifact_sha256":ACCEPTED_NATIVE_ARTIFACT_SHA256})
}
pub fn validate(root: &Path, name: &str, expected: &str) -> Result<()> {
    require(
        !name.is_empty()
            && Path::new(name)
                .components()
                .all(|c| matches!(c, Component::Normal(_))),
        "unsafe deleter source pin",
    )?;
    let path = root.join(name);
    if let Some((_, original)) = RETIRED.iter().find(|(old, _)| *old == name) {
        require(
            expected == *original,
            "historical deleter helper pin changed",
        )?;
        accepted()?;
        return match fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            _ => Err(format!("retired deleter helper must be absent: {name}").into()),
        };
    }
    let meta = fs::symlink_metadata(&path)
        .map_err(|e| format!("retained deleter source missing: {name}: {e}"))?;
    require(
        meta.is_file() && !meta.file_type().is_symlink(),
        "retained deleter source is not a regular file",
    )?;
    require(
        hash(&bytes(&path, 8 << 20)?) == expected,
        &format!("retained deleter source changed: {name}"),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    #[test]
    fn only_four_exact_historical_deleter_helpers_may_be_absent() {
        accepted().expect("fill actual accepted native proof before applying retirement patch");
        let root = tempfile::tempdir().unwrap();
        for &(name, pin) in RETIRED {
            assert!(validate(root.path(), name, pin).is_ok());
            assert!(validate(root.path(), name, &"0".repeat(64)).is_err());
            let path = root.path().join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, b"reintroduced source").unwrap();
            assert!(validate(root.path(), name, pin).is_err());
            fs::remove_file(&path).unwrap();
            symlink("nonexistent-original-deleter-helper", &path).unwrap();
            assert!(validate(root.path(), name, pin).is_err());
            fs::remove_file(path).unwrap();
        }
    }
    #[test]
    fn retained_runtime_missing_changed_symlink_and_unsafe_paths_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        let name = "openpilot/system/loggerd/deleter.py";
        let pin = hash(b"independent retained source");
        assert!(validate(root.path(), name, &pin).is_err());
        let path = root.path().join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"independent retained source").unwrap();
        assert!(validate(root.path(), name, &pin).is_ok());
        fs::write(&path, b"changed").unwrap();
        assert!(validate(root.path(), name, &pin).is_err());
        fs::remove_file(&path).unwrap();
        let target = root.path().join("target");
        fs::write(&target, b"independent retained source").unwrap();
        symlink(target, &path).unwrap();
        assert!(validate(root.path(), name, &pin).is_err());
        for name in [
            "rust/tools/unlisted.py",
            "../outside.py",
            "/absolute.py",
            "",
        ] {
            assert!(validate(root.path(), name, &pin).is_err());
        }
    }
}
