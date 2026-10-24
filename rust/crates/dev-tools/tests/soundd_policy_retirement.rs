//! Only the exact retired policy tool may be absent; live source seals remain mandatory.
#![cfg(feature = "soundd-policy-native")]
use openpilot_dev_tools::soundd_policy_reference as reference;
use std::{
    fs,
    path::{Path, PathBuf},
};
const RETIRED: &str = "rust/tools/check_soundd.py";
const PIN: &str = "3ec6eb74836e2dca07fbe4bc16e80a2f4d4c20af47bf49a9a4dec0e886411f9b";
const LIVE: [&str; 6] = [
    "rust/tools/original_params_binding.py",
    "openpilot/selfdrive/ui/soundd.py",
    "openpilot/common/filter_simple.py",
    "openpilot/selfdrive/car/openpilot_toggle.py",
    "openpilot/common/utils.py",
    "openpilot/system/micd.py",
];
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn copy() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    for name in LIVE {
        let path = temp.path().join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::copy(root().join(name), path).unwrap();
    }
    temp
}
#[test]
fn exact_retired_absence_has_separate_historical_provenance() {
    let temp = copy();
    let live = reference::sources(temp.path()).unwrap();
    assert_eq!(live.len(), 6);
    assert!(!live.contains_key(RETIRED));
    let historical = reference::historical_sources();
    assert_eq!(historical.len(), 1);
    assert_eq!(historical[RETIRED], PIN);
}
#[test]
fn changed_dangling_and_directory_reintroductions_are_rejected() {
    for mode in ["changed", "dangling", "directory"] {
        let temp = copy();
        let path = temp.path().join(RETIRED);
        match mode {
            "changed" => fs::write(&path, b"changed retired source").unwrap(),
            "dangling" => std::os::unix::fs::symlink(temp.path().join("missing"), &path).unwrap(),
            "directory" => fs::create_dir(&path).unwrap(),
            _ => unreachable!(),
        }
        assert!(reference::sources(temp.path()).is_err(), "{mode}");
    }
}
#[test]
fn every_retained_input_remains_required_and_pinned() {
    for name in LIVE {
        for changed in [false, true] {
            let temp = copy();
            let path = temp.path().join(name);
            if changed {
                fs::write(&path, b"changed live source").unwrap();
            } else {
                fs::remove_file(&path).unwrap();
            }
            assert!(
                reference::sources(temp.path()).is_err(),
                "{name} changed={changed}"
            );
        }
    }
}
