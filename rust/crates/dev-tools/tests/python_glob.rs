use openpilot_dev_tools::python_glob::glob;
use std::{
    ffi::OsString,
    fs,
    os::unix::{
        ffi::OsStringExt,
        fs::{symlink, PermissionsExt},
    },
    path::Path,
};
#[test]
fn python312_initial_path_errors_and_final_unstatted_entries() {
    assert_ne!(rustix::process::geteuid().as_raw(), 0);
    let t = tempfile::tempdir().unwrap();
    let root = t.path();
    // Independently observed pinned Python 3.12.15: Path('base\0').is_dir() is false.
    assert!(glob(
        root,
        Path::new(&OsString::from_vec(b"base\0".to_vec())),
        "*"
    )
    .unwrap()
    .is_empty());
    symlink("loop", root.join("loop")).unwrap();
    assert!(glob(root, Path::new("loop"), "*").unwrap().is_empty());
    fs::write(root.join("file"), b"").unwrap();
    assert!(glob(root, Path::new("file/child"), "*").unwrap().is_empty());
    fs::create_dir(root.join("denied")).unwrap();
    fs::set_permissions(root.join("denied"), fs::Permissions::from_mode(0o0)).unwrap();
    let result = glob(root, Path::new("denied/child"), "*");
    fs::set_permissions(root.join("denied"), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(result.is_err());
    symlink("absent", root.join("dangling")).unwrap();
    assert_eq!(
        glob(root, Path::new("."), "dangling").unwrap(),
        vec![Path::new(".").join("dangling")]
    );
}
