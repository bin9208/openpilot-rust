use openpilot_params::{metadata, Params, CLEAR_ON_MANAGER_START};
use std::{fs, os::unix::fs::symlink, sync::Arc, thread};

#[test]
fn raw_values_validation_and_native_layout() {
    let root = tempfile::tempdir().unwrap();
    let params = Params::open(root.path(), "d").unwrap();
    assert!(root.path().join("d").is_symlink());
    assert_eq!(params.get("CarParams").unwrap(), None);
    let binary = [0, 255, 254, 1, 0];
    params.put("CarParams", &binary).unwrap();
    assert_eq!(params.get("CarParams").unwrap(), Some(binary.to_vec()));
    assert_eq!(fs::read(root.path().join("d/CarParams")).unwrap(), binary);
    params.put("CarParams", b"").unwrap();
    assert_eq!(params.get("CarParams").unwrap(), Some(vec![]));
    params.remove("CarParams").unwrap();
    assert_eq!(params.get("CarParams").unwrap(), None);
    assert!(params.remove("CarParams").is_err());
    for key in ["Unknown", "../CarParams", "CarParams/x", "", "/CarParams"] {
        assert!(params.put(key, b"no").is_err());
        assert!(params.get(key).is_err());
        assert!(params.remove(key).is_err());
    }
    for prefix in ["", "..", "/d", "a/b"] {
        assert!(Params::open(root.path(), prefix).is_err());
    }
    assert_eq!(metadata("IsMetric").unwrap().default, Some("1"));
    assert_eq!(
        metadata("LongitudinalPersonality").unwrap().default,
        Some("1")
    );
    assert!(metadata("CarParams").unwrap().flags & CLEAR_ON_MANAGER_START != 0);
}

#[test]
fn existing_native_symlink_and_clear_mask_interoperate() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join(".tmp_native");
    fs::create_dir(&directory).unwrap();
    symlink(&directory, root.path().join("d")).unwrap();
    let params = Params::open(root.path(), "d").unwrap();
    params.put("CarParams", b"transient").unwrap();
    params.put("IsMetric", b"1").unwrap();
    fs::write(directory.join("UnknownOldKey"), b"old").unwrap();
    fs::create_dir(directory.join("UnknownDirectory")).unwrap();
    params.clear(CLEAR_ON_MANAGER_START).unwrap();
    assert_eq!(params.get("CarParams").unwrap(), None);
    assert_eq!(params.get("IsMetric").unwrap(), Some(b"1".to_vec()));
    assert!(!directory.join("UnknownOldKey").exists());
    assert!(directory.join("UnknownDirectory").is_dir());
}

#[test]
fn concurrent_writers_publish_only_complete_values() {
    let root = tempfile::tempdir().unwrap();
    let params = Arc::new(Params::open(root.path(), "d").unwrap());
    params.put("CarParams", &vec![1; 8192]).unwrap();
    thread::scope(|scope| {
        for value in 2..5 {
            let params = Arc::clone(&params);
            scope.spawn(move || {
                for _ in 0..25 {
                    params.put("CarParams", &vec![value; 8192]).unwrap();
                }
            });
        }
        for _ in 0..100 {
            let value = params.get("CarParams").unwrap().unwrap();
            assert_eq!(value.len(), 8192);
            assert!(value.iter().all(|byte| *byte == value[0]));
        }
    });
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 3);
}
