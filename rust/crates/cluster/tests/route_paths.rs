use openpilot_cluster::route_replay::paths::{
    adjacent_route_log_path, discover_route_logs, segment_index_from_name,
};
use std::fs;

#[test]
fn route_discovery_filters_route_and_segment_and_prefers_zstd_for_adjacent() {
    let root = tempfile::tempdir().unwrap();
    for name in ["aaa--000", "aaa--001", "aaa--002", "bbb--001"] {
        let folder = root.path().join(name);
        fs::create_dir(&folder).unwrap();
        fs::write(folder.join("rlog.zst"), []).unwrap();
    }
    fs::write(root.path().join("aaa--002/rlog.bz2"), []).unwrap();
    let discovered =
        discover_route_logs(&root.path().join("aaa--001"), "rlog", None, None).unwrap();
    assert_eq!(
        discovered,
        vec![
            root.path().join("aaa--001/rlog.zst"),
            root.path().join("aaa--002/rlog.zst")
        ]
    );
    let next = adjacent_route_log_path(&discovered[0], 1, "rlog").unwrap();
    assert_eq!(next, Some(root.path().join("aaa--002/rlog.zst")));
    let limited = discover_route_logs(root.path(), "rlog", Some(1), Some(2)).unwrap();
    assert_eq!(limited, discovered);
    assert!(adjacent_route_log_path(&discovered[0], 0, "rlog").is_err());
    assert!(discover_route_logs(root.path(), "missing", None, None).is_err());
    assert_eq!(segment_index_from_name("aaa--0009"), Some(9));
    assert_eq!(segment_index_from_name("aaa---1"), Some(1));
    assert_eq!(segment_index_from_name("aaa--+1"), None);
}
