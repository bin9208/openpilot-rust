//! Independent retirement key set and source-drift checks for both guard APIs.
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, path::Path};
pub fn verify(
    root: &Path,
    pins: &Map<String, Value>,
    check: impl Fn(&Path) -> openpilot_dev_tools::seal::Result<()>,
) {
    let bytes = include_bytes!("../fixtures/timed-tools/retired-source-pins.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "1f2b66f36db7b4ccf6ba536dff65a4fa9b5a4dd61a71d144bd94affe00906079"
    );
    let captured: Value = serde_json::from_slice(bytes).unwrap();
    let original_retired = captured["retired_sources"].as_object().unwrap();
    assert_eq!(original_retired.len(), 13);
    let retired = original_retired.keys().cloned().collect::<BTreeSet<_>>();
    for (name, sha) in original_retired {
        assert_eq!(pins.get(name), Some(sha), "retired pin changed: {name}");
    }
    check(root).unwrap();
    let copied = tempfile::tempdir().unwrap();
    for name in pins.keys() {
        let path = copied.path().join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        match fs::read(root.join(name)) {
            Ok(bytes) => fs::write(path, bytes).unwrap(),
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound && retired.contains(name) => {}
            Err(error) => panic!("read mandatory original {name}: {error}"),
        }
    }
    check(copied.path()).unwrap();
    for name in pins.keys() {
        let path = copied.path().join(name);
        let previous = fs::read(&path).ok();
        fs::write(&path, b"changed or reintroduced source").unwrap();
        assert!(check(copied.path()).is_err(), "changed {name} admitted");
        fs::remove_file(&path).unwrap();
        if retired.contains(name) {
            check(copied.path()).unwrap();
        } else {
            assert!(
                check(copied.path()).is_err(),
                "missing live {name} admitted"
            );
        }
        if let Some(bytes) = previous {
            fs::write(path, bytes).unwrap();
        }
    }
    // All thirteen may be absent together. Their absence must not disable
    // validation of any remaining required source.
    for name in &retired {
        match fs::remove_file(copied.path().join(name)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("remove retired {name}: {error}"),
        }
    }
    check(copied.path()).unwrap();
    for name in &retired {
        let path = copied.path().join(name);
        fs::write(&path, b"changed reintroduced retired source").unwrap();
        assert!(
            check(copied.path()).is_err(),
            "changed retired {name} admitted"
        );
        fs::remove_file(path).unwrap();
        check(copied.path()).unwrap();
    }
    for name in pins.keys().filter(|name| !retired.contains(*name)) {
        let path = copied.path().join(name);
        let bytes = fs::read(&path).unwrap();
        fs::write(&path, b"changed mandatory source").unwrap();
        assert!(
            check(copied.path()).is_err(),
            "changed live {name} with retired absent admitted"
        );
        fs::remove_file(&path).unwrap();
        assert!(
            check(copied.path()).is_err(),
            "missing live {name} with retired absent admitted"
        );
        fs::write(path, bytes).unwrap();
        check(copied.path()).unwrap();
    }
}
