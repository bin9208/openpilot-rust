use openpilot_dashcam_upload::{
    catalog,
    state::{Clock, Job},
};
use std::{fs, os::unix::fs::symlink};
#[test]
fn all_files_keeps_required_log_and_selects_only_regular_files_in_name_order() {
    let root = tempfile::tempdir().unwrap();
    for (name, bytes) in [
        ("rlog.zst", b"rlog".as_slice()),
        ("qcamera.ts", b"camera"),
        ("qcamera.mp4", b"derived"),
        ("qlog.zst", b"reduced"),
        ("aux.bin", b""),
    ] {
        fs::write(root.path().join(name), bytes).unwrap();
    }
    fs::create_dir(root.path().join("nested")).unwrap();
    symlink("rlog.zst", root.path().join("link")).unwrap();
    assert_eq!(
        catalog::file_summary(root.path())
            .unwrap()
            .iter()
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>(),
        ["qcamera.ts", "rlog.zst"]
    );
    let all = catalog::file_summary_scoped(root.path(), true).unwrap();
    assert_eq!(
        all.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
        [
            "aux.bin",
            "qcamera.mp4",
            "qcamera.ts",
            "qlog.zst",
            "rlog.zst"
        ]
    );
    assert_eq!(all[0].kind, "aux");
    assert_eq!(all[0].size, 0);
    fs::remove_file(root.path().join("rlog.zst")).unwrap();
    assert!(catalog::file_summary_scoped(root.path(), true).is_err());
}
#[test]
fn job_snapshot_keeps_scope_per_job() {
    let clock = Clock {
        wall: 1.0,
        monotonic: 1.0,
    };
    let default = Job::new("default".into(), vec![], clock);
    let all = Job::new_scoped("all".into(), vec![], clock, true);
    assert_eq!(default.snapshot().unwrap()["includeAllFiles"], false);
    assert_eq!(all.snapshot().unwrap()["includeAllFiles"], true);
}
