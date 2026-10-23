use openpilot_deleter::Deleter;
use openpilot_uploader::{
    clear_locks, EventSink, Transfer, TransferError, UploadResponse, Uploader, XattrCache,
};
use std::{collections::BTreeSet, fs, path::Path};

struct NoTransfer;
impl Transfer for NoTransfer {
    fn upload(
        &mut self,
        _: &Path,
        _: &Path,
        _: &mut dyn EventSink,
    ) -> Result<UploadResponse, TransferError> {
        panic!("scanner regression must not contact the network")
    }
}

#[test]
fn completed_video_failure_excludes_only_that_file_and_allows_segment_reclamation() {
    let root = tempfile::tempdir().unwrap();
    let segment = root.path().join("route--0");
    fs::create_dir(&segment).unwrap();
    for name in [
        "qlog.zst",
        "rlog.zst",
        "ecamera.hevc",
        "fcamera.hevc",
        "qcamera.ts",
    ] {
        fs::write(segment.join(name), b"fixture bytes").unwrap();
    }
    // A failed priority video and a failed raw video must both be excluded.
    fs::write(segment.join("qcamera.ts.failed"), []).unwrap();
    fs::write(segment.join("fcamera.hevc.failed"), []).unwrap();
    fs::write(segment.join("rlog.lock"), []).unwrap();
    let mut uploader = Uploader::new(
        root.path().to_owned(),
        NoTransfer,
        XattrCache::default(),
        Vec::new(),
    );
    assert!(uploader.list_upload_files(false, None).unwrap().is_empty());
    assert!(Deleter::new(root.path())
        .cycle(0, 0.0)
        .unwrap()
        .deleted
        .is_none());

    fs::remove_file(segment.join("rlog.lock")).unwrap();
    // Restart cleanup must not erase completed failure evidence.
    clear_locks(root.path(), &mut Vec::new()).unwrap();
    assert!(segment.join("fcamera.hevc.failed").exists());
    for metered in [false, true] {
        let names: BTreeSet<_> = uploader
            .list_upload_files(metered, Some("route"))
            .unwrap()
            .into_iter()
            .map(|file| file.name)
            .collect();
        let expected: BTreeSet<_> = ["qlog.zst", "rlog.zst", "ecamera.hevc"]
            .into_iter()
            .map(std::ffi::OsString::from)
            .collect();
        assert_eq!(names, expected);
        assert_eq!(
            uploader.next_file(metered, None).unwrap().unwrap().name,
            "qlog.zst"
        );
    }
    assert_eq!(
        Deleter::new(root.path())
            .cycle(0, 0.0)
            .unwrap()
            .deleted
            .as_deref(),
        Some(std::ffi::OsStr::new("route--0"))
    );
    assert!(!segment.exists());
}
