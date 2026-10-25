use super::*;
use crate::{TransferError, UploadResponse};

struct NoTransfer;
impl Transfer for NoTransfer {
    fn upload(
        &mut self,
        _: &Path,
        _: &Path,
        _: &mut dyn EventSink,
    ) -> Result<UploadResponse, TransferError> {
        panic!("cache scan must not upload")
    }
}
fn uploader(root: &Path) -> Uploader<NoTransfer, XattrCache, Vec<Event>> {
    Uploader::new(
        root.to_owned(),
        NoTransfer,
        XattrCache::default(),
        Vec::new(),
    )
}
fn mark(path: &Path) {
    rustix::fs::setxattr(path, "user.upload", b"1", rustix::fs::XattrFlags::empty()).unwrap();
}

#[test]
fn complete_scans_release_removed_segment_keys_and_old_map_capacity() {
    let root = tempfile::tempdir().unwrap();
    let mut upload = uploader(root.path());
    for generation in 0..3 {
        for index in 0..256 {
            let directory = root.path().join(format!("route-{generation}--{index}"));
            fs::create_dir(&directory).unwrap();
            let path = directory.join("qlog.zst");
            fs::write(&path, b"log").unwrap();
            if index % 2 == 0 {
                mark(&path);
            }
        }
        assert_eq!(upload.list_upload_files(false, None).unwrap().len(), 128);
        assert_eq!(upload.attributes.values.len(), 256);
        for entry in fs::read_dir(root.path()).unwrap() {
            fs::remove_dir_all(entry.unwrap().path()).unwrap();
        }
        assert!(upload.list_upload_files(false, None).unwrap().is_empty());
        assert!(upload.attributes.values.is_empty());
        assert!(upload.attributes.values.capacity() <= 64);
    }
}

#[test]
fn observed_file_deletion_refreshes_recreated_path_but_preserves_live_cache() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("route--0");
    fs::create_dir(&dir).unwrap();
    let deleted = dir.join("qlog.zst");
    let live = dir.join("qcamera.ts");
    fs::write(&deleted, b"log").unwrap();
    fs::write(&live, b"video").unwrap();
    mark(&deleted);
    let mut upload = uploader(root.path());
    assert_eq!(upload.list_upload_files(false, None).unwrap().len(), 1);
    mark(&live); // Existing paths retain the established cached-None policy.
    fs::remove_file(&deleted).unwrap();
    let selected = upload.list_upload_files(false, None).unwrap();
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].path, live);
    assert!(!upload.attributes.values.contains_key(&deleted));
    fs::write(&deleted, b"replacement without uploaded attribute").unwrap();
    let selected = upload.list_upload_files(false, None).unwrap();
    assert_eq!(selected.len(), 2);
    assert_eq!(selected[0].path, deleted);
    assert_eq!(upload.attributes.values[&live], None);
}

#[test]
fn locked_uncertain_and_other_root_entries_remain_owned_until_proven_absent() {
    let root = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let foreign = other.path().join("qlog.zst");
    fs::write(&foreign, b"foreign").unwrap();
    let dir = root.path().join("route--0");
    fs::create_dir(&dir).unwrap();
    let path = dir.join("qlog.zst");
    fs::write(&path, b"log").unwrap();
    let mut upload = uploader(root.path());
    upload.attributes.get(&foreign).unwrap();
    upload.list_upload_files(false, None).unwrap();
    fs::write(dir.join("rlog.lock"), b"").unwrap();
    fs::remove_file(&path).unwrap();
    assert!(upload.list_upload_files(false, None).unwrap().is_empty());
    assert!(upload.attributes.values.contains_key(&path));
    // A present top-level entry that cannot be inventoried as a directory is
    // uncertain, not proof of an absent segment. Avoid permission/root fixtures.
    fs::remove_dir_all(&dir).unwrap();
    fs::write(&dir, b"temporarily non-directory").unwrap();
    assert!(upload.list_upload_files(false, None).unwrap().is_empty());
    assert!(upload.attributes.values.contains_key(&path));
    fs::remove_file(&dir).unwrap();
    let original_root = upload.root.clone();
    upload.root = root.path().join("missing");
    assert!(upload.list_upload_files(false, None).unwrap().is_empty());
    assert!(upload.attributes.values.contains_key(&path));
    upload.root = original_root;
    upload.list_upload_files(false, None).unwrap();
    assert!(!upload.attributes.values.contains_key(&path));
    assert!(upload.attributes.values.contains_key(&foreign));
}
