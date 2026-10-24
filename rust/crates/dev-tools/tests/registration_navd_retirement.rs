//! Exact helper-retirement controls based on immutable original inputs.
//! Copied roots contain retained inputs only; no retired source reconstruction.
#![cfg(feature = "navd-config-tools-native")]
use openpilot_dev_tools::{navd_config_check as navd, registration_check as registration};
use openpilot_log_format::JsonValue;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
};
const RETIRED: [&str; 4] = [
    "rust/tools/check_registration.py",
    "rust/tools/registration_source.py",
    "rust/tools/check_navd_config.py",
    "rust/tools/navd_config_source.py",
];
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap()
}
fn pins(nav: bool) -> BTreeMap<String, String> {
    let path = if nav {
        "rust/crates/dev-tools/tests/fixtures/navd-config/manifest.json"
    } else {
        "rust/crates/dev-tools/tests/fixtures/registration/manifest.json"
    };
    let m: Value = serde_json::from_slice(&fs::read(root().join(path)).unwrap()).unwrap();
    serde_json::from_value(
        m[if nav {
            "source_sha256"
        } else {
            "all_source_sha256"
        }]
        .clone(),
    )
    .unwrap()
}
fn retired(nav: bool) -> &'static [&'static str] {
    if nav {
        &RETIRED
    } else {
        &RETIRED[..2]
    }
}
fn copied(nav: bool) -> tempfile::TempDir {
    let t = tempfile::tempdir().unwrap();
    for (n, h) in pins(nav) {
        if retired(nav).contains(&n.as_str()) {
            continue;
        }
        let raw = fs::read(root().join(&n)).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(&raw)),
            h,
            "retained original {n}"
        );
        let to = t.path().join(&n);
        fs::create_dir_all(to.parent().unwrap()).unwrap();
        fs::write(to, raw).unwrap();
    }
    t
}
fn live(nav: bool, root: &Path) -> Result<JsonValue, Box<dyn std::error::Error>> {
    if nav {
        navd::validate_source(root)
    } else {
        registration::validate_source(root)
    }
}
fn object(v: JsonValue) -> Value {
    serde_json::from_str(&v.to_json().unwrap()).unwrap()
}
#[test]
fn only_exact_retired_helpers_may_be_absent_and_absent_hashes_are_not_reported_live() {
    for nav in [false, true] {
        let t = copied(nav);
        let all = pins(nav);
        // Genuine current-production RED: existing validate_source rejects this absence.
        let actual =
            object(live(nav, t.path()).expect("exact retired helper absence must be admitted"));
        let mut expected = all.clone();
        for n in retired(nav) {
            expected.remove(*n);
            assert!(
                actual.get(*n).is_none(),
                "historical helper falsely labelled live {n}"
            )
        }
        assert_eq!(actual, serde_json::to_value(expected).unwrap());
        for (n, h) in all
            .iter()
            .filter(|(n, _)| !retired(nav).contains(&n.as_str()))
        {
            let path = t.path().join(n);
            let raw = fs::read(&path).unwrap();
            fs::remove_file(&path).unwrap();
            assert!(live(nav, t.path()).is_err(), "retained missing {n}");
            fs::write(&path, b"drift").unwrap();
            assert!(live(nav, t.path()).is_err(), "retained changed {n}");
            fs::remove_file(&path).unwrap();
            symlink("unowned-missing-target", &path).unwrap();
            assert!(live(nav, t.path()).is_err(), "retained dangling {n}");
            fs::remove_file(&path).unwrap();
            fs::write(path, raw).unwrap();
            assert_eq!(
                format!("{:x}", Sha256::digest(fs::read(root().join(n)).unwrap())),
                *h
            );
        }
    }
}
#[test]
fn retired_reintroduction_is_still_hash_checked_and_dangling_nonregular_entries_are_not_absence() {
    for nav in [false, true] {
        let t = copied(nav);
        live(nav, t.path()).unwrap();
        for n in retired(nav) {
            let p = t.path().join(n);
            fs::write(&p, b"reintroduced drift").unwrap();
            assert!(
                live(nav, t.path()).is_err(),
                "changed retired reintroduction {n}"
            );
            fs::remove_file(&p).unwrap();
            symlink("missing-historical-target", &p).unwrap();
            assert!(
                live(nav, t.path()).is_err(),
                "dangling present retired entry {n}"
            );
            fs::remove_file(&p).unwrap();
            fs::create_dir(&p).unwrap();
            assert!(live(nav, t.path()).is_err(), "directory not absence {n}");
            fs::remove_dir(&p).unwrap();
        }
    }
}
fn required(n: &str) -> PathBuf {
    let p = PathBuf::from(
        std::env::var_os(n)
            .unwrap_or_else(|| panic!("prerequisite: {n} must name actual frozen current ELF")),
    );
    assert!(p.is_absolute() && p.is_file());
    p
}
fn history(nav: bool) -> Value {
    let all = pins(nav);
    serde_json::to_value(
        retired(nav)
            .iter()
            .map(|n| ((*n).to_owned(), all[*n].clone()))
            .collect::<BTreeMap<_, _>>(),
    )
    .unwrap()
}
fn copy_navd_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for e in fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        if e.file_type().unwrap().is_dir() {
            copy_navd_tree(&e.path(), &to.join(e.file_name()));
        } else if e.path().extension() == Some(std::ffi::OsStr::new("rs")) {
            fs::copy(e.path(), to.join(e.file_name())).unwrap();
        }
    }
}
#[test]
fn actual_receipts_separate_absent_historical_hashes_from_live_inputs_with_real_probes() {
    let t = tempfile::tempdir().unwrap();
    let reg = copied(false);
    let out = t.path().join("registration");
    let options = registration::Options {
        root: reg.path().into(),
        binary: required("REGISTRATION_BINARY"),
        output: out.clone(),
        runner: vec![],
        case: Some("no_keys_overrides_saved".into()),
        suite: registration::Suite::All,
        environment: std::env::vars_os().collect(),
    };
    let result =
        object(registration::run(&options, &required("REGISTRATION_WORKER_BINARY")).unwrap());
    assert_eq!(result["result"], "PASS");
    let provenance: Value =
        serde_json::from_slice(&fs::read(out.join("provenance.json")).unwrap()).unwrap();
    assert_eq!(provenance["historical_source_sha256"], history(false));
    for n in retired(false) {
        assert!(provenance["source_sha256"].get(*n).is_none());
    }
    assert_eq!(provenance["source_sha256"].as_object().unwrap().len(), 45);
    let nav = copied(true);
    copy_navd_tree(
        &root().join("rust/crates/navd"),
        &nav.path().join("rust/crates/navd"),
    );
    let options = navd::Options {
        root: nav.path().into(),
        binary: required("NAVD_CONFIG_BINARY"),
        output: t.path().join("navd"),
        environment: std::env::vars_os().collect(),
    };
    let report = object(navd::run(&options, &required("NAVD_CONFIG_WORKER_BINARY")).unwrap());
    assert_eq!(report["status"], "PASS");
    assert_eq!(report["cases"].as_array().unwrap().len(), 26);
    assert_eq!(report["historical_source_sha256"], history(true));
    for n in retired(true) {
        assert!(report["files"].get(*n).is_none());
    }
}
