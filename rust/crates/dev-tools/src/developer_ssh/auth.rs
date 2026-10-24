//! Explicit-root auth_config operations; original repository MIT license applies.
//! Copyright (c) 2018, Comma.ai, Inc.
use openpilot_log_format::JsonValue;
use std::{
    io::Write,
    path::{Path, PathBuf},
};

pub fn config_root(home: &Path, prefix: &str, tici: bool) -> PathBuf {
    if tici {
        PathBuf::from("/tmp/.comma")
    } else {
        home.join(format!(".comma{prefix}"))
    }
}
/// Caller supplies a root so source tests use synthetic private directories only.
/// Missing file, invalid UTF-8/JSON, missing key and wrong top-level type all become None.
/// No truthiness/type validation is added to the returned access_token.
pub fn get_token_at(root: &Path) -> JsonValue {
    std::fs::read_to_string(root.join("auth.json"))
        .ok()
        .and_then(|text| JsonValue::parse(&text).ok())
        .and_then(|auth| auth.get("access_token"))
        .unwrap_or_else(|| JsonValue::parse("null").expect("JSON null"))
}

/// Preserve mkdir(exist_ok=True), open("w") truncation and default JSON layout.
/// Follows directory/file symlinks like the source; callers supply the root.
/// Accepts the existing JsonValue domain, not arbitrary Python objects/cycles.
pub fn set_token_at(root: &Path, token: &JsonValue) -> super::Result<()> {
    // Rust's recursive mkdir accepts an empty path; Python's makedirs does not.
    if root.as_os_str().is_empty() {
        std::fs::create_dir(root)?;
    } else {
        std::fs::create_dir_all(root)?;
    }
    let mut file = std::fs::File::create(root.join("auth.json"))?;
    // Open/truncate before encoding, as in the original json.dump context.
    file.write_all(b"{\"access_token\": ")?;
    file.write_all(token.to_json()?.as_bytes())?;
    file.write_all(b"}")?;
    Ok(())
}

/// Missing auth.json (including a missing parent) is the only suppressed I/O error.
/// Unlink the leaf itself; do not follow a leaf symlink or remove the root.
pub fn clear_token_at(root: &Path) -> super::Result<()> {
    match std::fs::remove_file(root.join("auth.json")) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::{clear_token_at, get_token_at, set_token_at};
    use openpilot_log_format::JsonValue;
    use std::{
        fs,
        io::ErrorKind,
        os::unix::{
            ffi::OsStringExt,
            fs::{symlink, MetadataExt, PermissionsExt},
        },
        path::Path,
    };

    fn value(source: &str) -> JsonValue {
        JsonValue::parse(source).unwrap()
    }

    fn written(root: &Path, token: &JsonValue, expected: &str) {
        set_token_at(root, token).unwrap();
        assert_eq!(
            fs::read(root.join("auth.json")).unwrap(),
            expected.as_bytes()
        );
    }

    fn io_kind(error: Box<dyn std::error::Error>) -> ErrorKind {
        error.downcast::<std::io::Error>().unwrap().kind()
    }

    #[test]
    fn set_creates_nested_private_root_and_existing_root_is_allowed() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("nested/config");
        written(
            &root,
            &JsonValue::text("inert-only"),
            r#"{"access_token": "inert-only"}"#,
        );
        written(&root, &value("null"), r#"{"access_token": null}"#);
        assert_eq!(fs::read_dir(root).unwrap().count(), 1);
    }

    #[test]
    fn set_truncates_in_place_preserving_existing_inode_mode_and_hardlink() {
        let temp = tempfile::tempdir().unwrap();
        let auth = temp.path().join("auth.json");
        let alias = temp.path().join("hardlink");
        fs::write(&auth, b"synthetic old content with a long trailing suffix").unwrap();
        fs::set_permissions(&auth, fs::Permissions::from_mode(0o640)).unwrap();
        fs::hard_link(&auth, &alias).unwrap();
        let before = fs::metadata(&auth).unwrap();
        written(temp.path(), &value("0"), r#"{"access_token": 0}"#);
        let after = fs::metadata(&auth).unwrap();
        assert_eq!((before.dev(), before.ino()), (after.dev(), after.ino()));
        assert_eq!(after.mode() & 0o777, 0o640);
        assert_eq!(fs::read(alias).unwrap(), b"{\"access_token\": 0}");
    }

    #[test]
    fn set_uses_ascii_escapes_default_separators_and_no_newline() {
        let temp = tempfile::tempdir().unwrap();
        written(
            temp.path(),
            &JsonValue::text("\"\\/\u{0}\u{8}\t\n\u{c}\r\u{1f}\u{7f}é한😀"),
            r#"{"access_token": "\"\\/\u0000\b\t\n\f\r\u001f\u007f\u00e9\ud55c\ud83d\ude00"}"#,
        );
    }

    #[test]
    fn set_preserves_raw_lone_surrogates_without_utf8_replacement() {
        let temp = tempfile::tempdir().unwrap();
        let token = JsonValue::codepoints(vec![0xd800, 65, 0xdcff, 0xd800, 0xdc00]).unwrap();
        written(
            temp.path(),
            &token,
            r#"{"access_token": "\ud800A\udcff\ud800\udc00"}"#,
        );
    }

    #[test]
    fn set_accepts_full_representable_json_values_and_object_order() {
        let temp = tempfile::tempdir().unwrap();
        for (source, expected) in [
            ("null", r#"{"access_token": null}"#),
            ("false", r#"{"access_token": false}"#),
            ("true", r#"{"access_token": true}"#),
            (r#""""#, r#"{"access_token": ""}"#),
            ("[]", r#"{"access_token": []}"#),
            ("{}", r#"{"access_token": {}}"#),
            (
                r#"{"z":[1,false,null],"a":{"x":"synthetic"},"z":[2]}"#,
                r#"{"access_token": {"z": [2], "a": {"x": "synthetic"}}}"#,
            ),
        ] {
            written(temp.path(), &value(source), expected);
        }
    }

    #[test]
    fn set_keeps_python_nonfinite_and_signed_zero_float_tokens() {
        let temp = tempfile::tempdir().unwrap();
        written(
            temp.path(),
            &value("[NaN,Infinity,-Infinity,-0.0,1.0,1e-7,1e16,0.0001]"),
            r#"{"access_token": [NaN, Infinity, -Infinity, -0.0, 1.0, 1e-07, 1e+16, 0.0001]}"#,
        );
    }

    #[test]
    fn set_keeps_big_integer_digits_through_existing_parser_limit() {
        let temp = tempfile::tempdir().unwrap();
        let digits = "7".repeat(4300);
        written(
            temp.path(),
            &value(&digits),
            &format!("{{\"access_token\": {digits}}}"),
        );
        written(
            temp.path(),
            &value(&format!("-{digits}")),
            &format!("{{\"access_token\": -{digits}}}"),
        );
    }

    #[test]
    fn set_follows_private_root_directory_alias() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("target");
        fs::create_dir(&target).unwrap();
        let alias = temp.path().join("root-alias");
        symlink(&target, &alias).unwrap();
        written(
            &alias,
            &JsonValue::text("synthetic"),
            r#"{"access_token": "synthetic"}"#,
        );
        assert!(fs::symlink_metadata(alias)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(
            fs::read(target.join("auth.json")).unwrap(),
            b"{\"access_token\": \"synthetic\"}"
        );
    }

    #[test]
    fn set_follows_auth_leaf_alias_and_truncates_referent_in_place() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        fs::create_dir(&root).unwrap();
        let target = temp.path().join("referent");
        fs::write(&target, b"synthetic longer previous content").unwrap();
        symlink("../referent", root.join("auth.json")).unwrap();
        let before = fs::metadata(&target).unwrap();
        written(&root, &value("false"), r#"{"access_token": false}"#);
        assert!(fs::symlink_metadata(root.join("auth.json"))
            .unwrap()
            .file_type()
            .is_symlink());
        let after = fs::metadata(&target).unwrap();
        assert_eq!((before.dev(), before.ino()), (after.dev(), after.ino()));
        assert_eq!(fs::read(target).unwrap(), b"{\"access_token\": false}");
    }

    #[test]
    fn set_follows_dangling_auth_alias_to_new_private_referent() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        fs::create_dir(&root).unwrap();
        let target = temp.path().join("new-referent");
        symlink(&target, root.join("auth.json")).unwrap();
        written(&root, &value("true"), r#"{"access_token": true}"#);
        assert!(fs::symlink_metadata(root.join("auth.json"))
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(fs::read(target).unwrap(), b"{\"access_token\": true}");
    }

    #[test]
    fn set_propagates_root_file_broken_root_and_auth_directory_errors() {
        let temp = tempfile::tempdir().unwrap();
        let root_file = temp.path().join("root-file");
        fs::write(&root_file, b"inert root sentinel").unwrap();
        assert!(set_token_at(&root_file, &value("null")).is_err());
        assert!(set_token_at(&root_file.join("child"), &value("null")).is_err());
        assert_eq!(fs::read(&root_file).unwrap(), b"inert root sentinel");
        let broken = temp.path().join("broken-root");
        let missing = temp.path().join("missing-target");
        symlink(&missing, &broken).unwrap();
        assert!(set_token_at(&broken, &value("null")).is_err());
        assert!(!missing.exists());
        let root = temp.path().join("auth-directory-root");
        fs::create_dir_all(root.join("auth.json")).unwrap();
        fs::write(root.join("auth.json/sentinel"), b"inert").unwrap();
        assert!(set_token_at(&root, &value("null")).is_err());
        assert_eq!(fs::read(root.join("auth.json/sentinel")).unwrap(), b"inert");
    }

    #[test]
    fn clear_removes_only_auth_leaf_and_keeps_root_and_other_files() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join("auth.json"),
            b"inert deliberately invalid JSON",
        )
        .unwrap();
        fs::write(temp.path().join("sentinel"), b"inert neighbor").unwrap();
        clear_token_at(temp.path()).unwrap();
        clear_token_at(temp.path()).unwrap();
        assert!(temp.path().is_dir());
        assert!(!temp.path().join("auth.json").exists());
        assert_eq!(
            fs::read(temp.path().join("sentinel")).unwrap(),
            b"inert neighbor"
        );
    }

    #[test]
    fn clear_tolerates_missing_file_parent_and_dangling_root() {
        let temp = tempfile::tempdir().unwrap();
        clear_token_at(temp.path()).unwrap();
        clear_token_at(&temp.path().join("absent/child")).unwrap();
        let alias = temp.path().join("root-alias");
        symlink(temp.path().join("absent"), &alias).unwrap();
        clear_token_at(&alias).unwrap();
        assert!(fs::symlink_metadata(alias)
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(!temp.path().join("absent").exists());
    }

    #[test]
    fn clear_unlinks_real_and_dangling_auth_aliases_without_following_leaf() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("referent");
        fs::write(&target, b"inert referent must remain").unwrap();
        let auth = temp.path().join("auth.json");
        symlink(&target, &auth).unwrap();
        clear_token_at(temp.path()).unwrap();
        assert!(fs::symlink_metadata(&auth).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"inert referent must remain");
        let missing = temp.path().join("missing-target");
        symlink(&missing, &auth).unwrap();
        clear_token_at(temp.path()).unwrap();
        assert!(fs::symlink_metadata(auth).is_err());
        assert!(!missing.exists());
    }

    #[test]
    fn clear_follows_private_parent_alias_but_keeps_parent_alias() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("target");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("auth.json"), b"inert").unwrap();
        let alias = temp.path().join("root-alias");
        symlink(&target, &alias).unwrap();
        clear_token_at(&alias).unwrap();
        assert!(!target.join("auth.json").exists());
        assert!(fs::symlink_metadata(alias)
            .unwrap()
            .file_type()
            .is_symlink());
    }

    #[test]
    fn clear_propagates_directory_not_directory_and_loop_errors() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        fs::create_dir_all(root.join("auth.json")).unwrap();
        assert!(clear_token_at(&root).is_err());
        assert!(root.join("auth.json").is_dir());
        let file = temp.path().join("parent-file");
        fs::write(&file, b"inert").unwrap();
        assert_ne!(
            io_kind(clear_token_at(&file).unwrap_err()),
            ErrorKind::NotFound
        );
        let loop_root = temp.path().join("loop-root");
        symlink("loop-root", &loop_root).unwrap();
        assert_ne!(
            io_kind(clear_token_at(&loop_root).unwrap_err()),
            ErrorKind::NotFound
        );
        assert!(fs::symlink_metadata(loop_root)
            .unwrap()
            .file_type()
            .is_symlink());
    }

    struct RestoreMode<'a>(&'a Path, u32);
    impl Drop for RestoreMode<'_> {
        fn drop(&mut self) {
            fs::set_permissions(self.0, fs::Permissions::from_mode(self.1)).unwrap();
        }
    }

    #[test]
    fn permission_errors_propagate_for_set_and_clear_on_unprivileged_runner() {
        // The focused gate requires non-root execution; never silently skip this case.
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("locked-root");
        fs::create_dir(&root).unwrap();
        let auth = root.join("auth.json");
        fs::write(&auth, b"inert preserved content").unwrap();
        let _restore_file = RestoreMode(&auth, 0o600);
        fs::set_permissions(&auth, fs::Permissions::from_mode(0o400)).unwrap();
        assert_eq!(
            io_kind(set_token_at(&root, &value("null")).unwrap_err()),
            ErrorKind::PermissionDenied
        );
        assert_eq!(fs::read(&auth).unwrap(), b"inert preserved content");
        let _restore_root = RestoreMode(&root, 0o700);
        fs::set_permissions(&root, fs::Permissions::from_mode(0o500)).unwrap();
        assert_eq!(
            io_kind(clear_token_at(&root).unwrap_err()),
            ErrorKind::PermissionDenied
        );
        assert!(auth.is_file());
    }

    #[test]
    fn explicit_nonunicode_private_root_roundtrip_and_clear_need_no_environment() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp
            .path()
            .join(std::ffi::OsString::from_vec(b"private-\xff".to_vec()));
        written(
            &root,
            &JsonValue::text("synthetic"),
            r#"{"access_token": "synthetic"}"#,
        );
        assert!(get_token_at(&root).text_eq("synthetic"));
        clear_token_at(&root).unwrap();
        assert!(matches!(
            get_token_at(&root).view(),
            openpilot_log_format::JsonView::Null
        ));
        assert!(root.is_dir());
    }
}
