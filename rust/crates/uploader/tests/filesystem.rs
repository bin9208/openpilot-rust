use openpilot_uploader::{
    clear_locks, Attributes, Candidate, Error, EventSink, Outcome, Transfer, TransferError,
    UploadResponse, Uploader, XattrCache,
};
use std::{fs, io, path::Path};
struct NoTransfer;
impl Transfer for NoTransfer {
    fn upload(
        &mut self,
        _: &Path,
        _: &Path,
        _: &mut dyn EventSink,
    ) -> Result<UploadResponse, TransferError> {
        panic!("unexpected network transfer")
    }
}
struct FailedTransfer;
impl Transfer for FailedTransfer {
    fn upload(
        &mut self,
        _: &Path,
        _: &Path,
        _: &mut dyn EventSink,
    ) -> Result<UploadResponse, TransferError> {
        Err(TransferError::Contract("fixture upload failure"))
    }
}

#[test]
fn opaque_filesystem_name_does_not_panic_while_reporting_transfer_failure() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    // Given a real log directory containing a byte that is not UTF-8.
    let root = tempfile::tempdir().unwrap();
    let directory = root
        .path()
        .join(OsString::from_vec(b"route-\xff--0".to_vec()));
    fs::create_dir(&directory).unwrap();
    fs::write(directory.join("qlog"), b"synthetic log").unwrap();
    let mut uploader = Uploader::new(
        root.path().to_owned(),
        FailedTransfer,
        XattrCache::default(),
        Vec::new(),
    );
    // When transfer fails, then diagnostics cannot turn the recoverable failure into a panic.
    assert_eq!(uploader.step(1, false, None).unwrap(), Outcome::Failure);
    assert_eq!(
        uploader.events.last().unwrap().name(),
        Some("upload_failed")
    );
    assert!(!uploader.last_filename.exists());
}
#[test]
fn cache_preserves_missing_and_positive_values_until_mark_invalidates_them() {
    // Given a real filesystem file whose missing attribute was cached.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("qlog");
    fs::write(&path, b"log").unwrap();
    let mut cache = XattrCache::default();
    assert_eq!(cache.get(&path).unwrap(), None);
    rustix::fs::setxattr(&path, "user.upload", b"1", rustix::fs::XattrFlags::empty()).unwrap();
    // When another process changes the attribute, then the uploader retains its cached result.
    assert_eq!(cache.get(&path).unwrap(), None);
    cache.mark_uploaded(&path).unwrap();
    assert_eq!(cache.get(&path).unwrap(), Some(b"1".to_vec()));
    rustix::fs::removexattr(&path, "user.upload").unwrap();
    assert_eq!(cache.get(&path).unwrap(), Some(b"1".to_vec()));
    // A failed write must also invalidate the old cache entry.
    fs::remove_file(&path).unwrap();
    assert!(cache.mark_uploaded(&path).is_err());
    fs::write(&path, b"new").unwrap();
    assert_eq!(cache.get(&path).unwrap(), None);
}
#[test]
fn startup_unlinks_locks_only_one_directory_deep_and_reports_entry_errors() {
    // Given root files, segment locks, and a nested lock.
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("route--0");
    fs::create_dir_all(dir.join("nested")).unwrap();
    for path in [
        root.path().join("root.lock"),
        dir.join("qlog.lock"),
        dir.join("nested/deep.lock"),
    ] {
        fs::write(path, b"").unwrap();
    }
    let mut events = Vec::new();
    // When normal startup clears locks, then only the direct segment lock disappears.
    clear_locks(root.path(), &mut events).unwrap();
    assert!(!dir.join("qlog.lock").exists());
    assert!(root.path().join("root.lock").exists());
    assert!(dir.join("nested/deep.lock").exists());
    assert_eq!(events.len(), 1);
    assert!(clear_locks(&root.path().join("missing"), &mut events).is_err());
}
struct FailMark;
impl Attributes for FailMark {
    fn get(&mut self, _: &Path) -> io::Result<Option<Vec<u8>>> {
        Ok(None)
    }
    fn mark_uploaded(&mut self, _: &Path) -> io::Result<()> {
        Err(io::Error::from_raw_os_error(13))
    }
}
#[test]
fn zero_size_file_mark_failure_preserves_original_fatal_branch() {
    // Given an empty upload with an unwritable extended attribute.
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("qlog");
    fs::write(&path, b"").unwrap();
    let mut uploader = Uploader::new(root.path().to_owned(), NoTransfer, FailMark, Vec::new());
    // When zero-size handling skips the transfer, then the source's uninitialized-last_exc error remains fatal.
    assert!(matches!(
        uploader.upload(
            &Candidate {
                name: "qlog".into(),
                key: "qlog".into(),
                path
            },
            1,
            false
        ),
        Err(Error::UninitializedLastException(_))
    ));
    assert!(uploader.last_filename.as_os_str().is_empty());
}
#[test]
fn zero_size_file_marks_without_changing_last_successful_filename() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("qlog");
    fs::write(&path, b"").unwrap();
    let mut uploader = Uploader::new(
        root.path().to_owned(),
        NoTransfer,
        XattrCache::default(),
        Vec::new(),
    );
    uploader.last_filename = "previous".into();
    assert_eq!(
        uploader
            .upload(
                &Candidate {
                    name: "qlog".into(),
                    key: "qlog".into(),
                    path: path.clone()
                },
                1,
                false
            )
            .unwrap(),
        Outcome::Success
    );
    assert_eq!(uploader.last_filename, Path::new("previous"));
    assert_eq!(uploader.attributes.get(&path).unwrap(), Some(b"1".to_vec()));
}
