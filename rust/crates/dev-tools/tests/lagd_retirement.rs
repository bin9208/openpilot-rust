//! Only the nine retired QA files may disappear; live reference inputs stay pinned.
#![cfg(feature = "lagd-tools-native")]
use openpilot_dev_tools::lagd_reference::{retired_source, sources};
use serde_json::Value;
use std::{fs, path::Path};
const RETIRED: [&str; 9] = [
    "rust/tools/check_lagd_numeric.py",
    "rust/tools/check_lagd_loop.py",
    "rust/tools/check_lagd_cache.py",
    "rust/tools/check_lagd_packet.py",
    "rust/tools/check_lagd_daemon.py",
    "rust/tools/check_lagd_params_io.py",
    "rust/tools/lagd_reference.py",
    "rust/tools/lagd_loop_reference.py",
    "rust/tools/lagd_frames.py",
];
#[test]
fn exactly_nine_originals_may_be_absent_but_present_originals_and_all_sixteen_live_inputs_are_pinned(
) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let manifest: Value =
        serde_json::from_str(include_str!("fixtures/lagd-tools/source-pins.json")).unwrap();
    let pins = manifest["sources"].as_object().unwrap();
    assert_eq!(pins.len(), 25);
    assert!(RETIRED.iter().all(|name| pins.contains_key(*name)));
    for name in RETIRED {
        assert!(retired_source(name));
    }
    for name in [
        "check_lagd_numeric.py",
        "rust/tools/check_lagd_numeric.py/extra",
        "other/rust/tools/lagd_frames.py",
        "rust/tools/lagd_frames.py.bak",
        "openpilot/selfdrive/locationd/lagd.py",
        "openpilot/cereal/log.capnp",
    ] {
        assert!(!retired_source(name), "overbroad retirement: {name}");
    }
    let temp = tempfile::tempdir().unwrap();
    for name in pins.keys() {
        let target = temp.path().join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        if root.join(name).exists() {
            fs::copy(root.join(name), target).unwrap();
        } else {
            assert!(
                RETIRED.contains(&name.as_str()),
                "missing live input {name}"
            );
        }
    }
    sources(temp.path()).unwrap();
    for name in RETIRED {
        let target = temp.path().join(name);
        if target.exists() {
            fs::remove_file(target).unwrap();
        }
    }
    sources(temp.path())
        .expect("the exact nine retired originals must not be execution dependencies");
    for name in RETIRED {
        let target = temp.path().join(name);
        fs::write(&target, b"changed reintroduced original").unwrap();
        assert!(sources(temp.path()).is_err(), "accepted changed {name}");
        fs::remove_file(&target).unwrap();
        std::os::unix::fs::symlink("missing-original-target", &target).unwrap();
        assert!(
            sources(temp.path()).is_err(),
            "accepted dangling original {name}"
        );
        fs::remove_file(&target).unwrap();
        fs::create_dir(&target).unwrap();
        assert!(
            sources(temp.path()).is_err(),
            "accepted original directory {name}"
        );
        fs::remove_dir(&target).unwrap();
        // The immutable Git-retired digest is tested whenever the real original is
        // available; absence after retirement never invents a replacement source.
        if root.join(name).exists() {
            fs::copy(root.join(name), &target).unwrap();
            sources(temp.path()).unwrap();
        }
        if target.exists() {
            fs::remove_file(target).unwrap();
        }
        sources(temp.path()).unwrap();
    }
    let mut retained = 0;
    for name in pins.keys().filter(|name| !RETIRED.contains(&name.as_str())) {
        retained += 1;
        let target = temp.path().join(name);
        let original = fs::read(&target).unwrap();
        fs::remove_file(&target).unwrap();
        assert!(
            sources(temp.path()).is_err(),
            "missing live input accepted: {name}"
        );
        fs::write(&target, b"changed live input").unwrap();
        assert!(
            sources(temp.path()).is_err(),
            "changed live input accepted: {name}"
        );
        fs::write(target, original).unwrap();
        sources(temp.path()).unwrap();
    }
    assert_eq!(retained, 16);
}
