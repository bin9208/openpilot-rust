use openpilot_uploader::{
    clear_locks, Attributes, Backoff, Candidate, Event, EventSink, Outcome, Transfer,
    TransferError, UploadResponse, Uploader, XattrCache,
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
struct FailMark {
    fail: bool,
    cache: XattrCache,
}
impl Attributes for FailMark {
    fn get(&mut self, path: &Path) -> io::Result<Option<Vec<u8>>> {
        self.cache.get(path)
    }
    fn mark_uploaded(&mut self, path: &Path) -> io::Result<()> {
        if self.fail {
            Err(io::Error::from_raw_os_error(13))
        } else {
            self.cache.mark_uploaded(path)
        }
    }
}
#[test]
fn skipped_file_mark_failure_retries_with_backoff_and_recovers_without_transfer() {
    for (name, size) in [("qlog", 0), ("qlog", 25_000_001), ("qcam", 5_000_001)] {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("boot");
        fs::create_dir(&directory).unwrap();
        let path = directory.join(name);
        fs::File::create(&path).unwrap().set_len(size).unwrap();
        let mut uploader = Uploader::new(
            root.path().to_owned(),
            NoTransfer,
            FailMark {
                fail: true,
                cache: XattrCache::default(),
            },
            Vec::new(),
        );
        uploader.last_filename = "previous".into();
        let mut backoff = Backoff::default();
        for delay in [0.2, 0.4] {
            let outcome = uploader
                .step(1, false, None)
                .expect("marking failure must not exit the daemon");
            assert_eq!(outcome, Outcome::Failure);
            assert_eq!(backoff.next(outcome, false, 0.0), delay);
            assert_eq!(uploader.attributes.get(&path).unwrap(), None);
            assert_eq!(uploader.next_file(false, None).unwrap().unwrap().path, path);
            assert_eq!(uploader.last_filename, Path::new("previous"));
            let Event::Fields { name, fields, .. } = uploader.events.last().unwrap() else {
                panic!("missing marking diagnostic")
            };
            assert_eq!(*name, "uploader_setxattr_failed");
            let details = fields["exc"]
                .as_array()
                .expect("actual xattr error details");
            assert!(details[0].as_str().unwrap().contains("PermissionDenied"));
            assert!(details[0].as_str().unwrap().contains("13"));
            assert_eq!(fields["sz"], size);
            assert!(!uploader
                .events
                .iter()
                .any(|event| event.name() == Some("upload_success")));
        }
        uploader.attributes.fail = false;
        let outcome = uploader.step(1, false, None).unwrap();
        assert_eq!(outcome, Outcome::Success);
        assert_eq!(backoff.next(outcome, false, 0.0), 0.1);
        assert_eq!(uploader.attributes.get(&path).unwrap(), Some(b"1".to_vec()));
        assert_eq!(uploader.step(1, false, None).unwrap(), Outcome::Idle);
        assert_eq!(uploader.last_filename, Path::new("previous"));
    }
}
struct SuccessfulTransfer;
impl Transfer for SuccessfulTransfer {
    fn upload(
        &mut self,
        _: &Path,
        _: &Path,
        _: &mut dyn EventSink,
    ) -> Result<UploadResponse, TransferError> {
        Ok(UploadResponse {
            status: 200,
            content_length: "3".into(),
        })
    }
}
#[test]
fn transferred_file_mark_failure_keeps_success_and_reports_actual_mark_error() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("qlog");
    fs::write(&path, b"log").unwrap();
    let mut uploader = Uploader::new(
        root.path().to_owned(),
        SuccessfulTransfer,
        FailMark {
            fail: true,
            cache: XattrCache::default(),
        },
        Vec::new(),
    );
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
    assert_eq!(uploader.last_filename, path);
    assert_eq!(uploader.attributes.get(&path).unwrap(), None);
    let Event::Fields { fields, .. } = uploader.events.last().unwrap() else {
        panic!("missing marking diagnostic")
    };
    assert!(fields["exc"][0]
        .as_str()
        .unwrap()
        .contains("PermissionDenied"));
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
