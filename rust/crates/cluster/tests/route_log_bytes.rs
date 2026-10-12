use openpilot_cluster::route_replay::read_log_bytes;
use std::fs;

#[test]
fn log_bytes_preserve_raw_and_decode_concatenated_original_bzip_streams_by_magic() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("owned.log");
    fs::write(&path, b"raw\0data").unwrap();
    assert_eq!(read_log_bytes(&path).unwrap(), b"raw\0data");
    let compressed = [
        66, 90, 104, 57, 49, 65, 89, 38, 83, 89, 178, 57, 11, 221, 0, 0, 0, 1, 128, 1, 32, 28, 0,
        32, 0, 33, 131, 65, 154, 2, 92, 113, 119, 36, 83, 133, 9, 11, 35, 144, 189, 208, 66, 90,
        104, 57, 49, 65, 89, 38, 83, 89, 38, 138, 2, 52, 0, 0, 2, 129, 128, 14, 1, 136, 0, 32, 0,
        34, 24, 104, 48, 7, 2, 152, 93, 201, 20, 225, 66, 64, 154, 40, 8, 208,
    ];
    fs::write(&path, compressed).unwrap();
    assert_eq!(read_log_bytes(&path).unwrap(), b"firstsecond");
    let mut trailing = compressed.to_vec();
    trailing.extend(b"trailing");
    fs::write(&path, trailing).unwrap();
    assert_eq!(read_log_bytes(&path).unwrap(), b"firstsecond");
    for name in ["empty.bz2", "empty.zst"] {
        let empty = root.path().join(name);
        fs::write(&empty, []).unwrap();
        assert!(read_log_bytes(&empty).unwrap().is_empty());
    }
    let suffix = root.path().join("invalid.bz2");
    fs::write(&suffix, b"not a bzip stream").unwrap();
    assert!(read_log_bytes(&suffix).is_err());
}
