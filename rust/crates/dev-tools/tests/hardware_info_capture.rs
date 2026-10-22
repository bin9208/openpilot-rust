#![cfg(target_os = "linux")]

use openpilot_dev_tools::hardware_info_capture::validate;
use serde_json::Value;
use std::{fs, path::PathBuf};

const RETIRED: [&str; 3] = [
    "rust/tools/check_hardware_info.py",
    "rust/tools/hardware_info_cases.py",
    "rust/tools/hardware_info_reference.py",
];

struct Admission {
    _temporary: tempfile::TempDir,
    root: PathBuf,
    capture: PathBuf,
    binary: PathBuf,
}

impl Admission {
    fn prepare() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("source");
        let capture = temporary.path().join("capture");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&capture).unwrap();
        let project = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .unwrap();
        let original = project.join("rust/crates/dev-tools/tests/fixtures/hardware-info-original");
        let manifest_bytes = fs::read(original.join("capture-manifest.json")).unwrap();
        let manifest: Value = serde_json::from_slice(&manifest_bytes).unwrap();
        fs::write(capture.join("capture-manifest.json"), manifest_bytes).unwrap();
        for name in manifest["files"].as_object().unwrap().keys() {
            let target = capture.join(name);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::copy(original.join(name), target).unwrap();
        }
        for name in manifest["source_sha256"].as_object().unwrap().keys() {
            if RETIRED.contains(&name.as_str()) {
                continue;
            }
            let target = root.join(name);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::copy(project.join(name), target).unwrap();
        }
        // Admission-only ELF input; this test never executes it as an API peer.
        let binary = temporary.path().join("admission-only-elf");
        fs::copy(std::env::current_exe().unwrap(), &binary).unwrap();
        Self {
            _temporary: temporary,
            root,
            capture,
            binary,
        }
    }

    fn admit(
        &self,
    ) -> openpilot_dev_tools::seal::Result<openpilot_dev_tools::hardware_info_capture::Receipt>
    {
        validate(
            &self.root,
            &self.capture,
            &self.capture.join("plans.json.gz"),
            true,
            &self.binary,
        )
    }
}

#[test]
fn authentic_original_capture_is_accepted_and_rechecked() {
    let data = Admission::prepare();
    let receipt = data.admit().unwrap();
    receipt.verify().unwrap();
    assert_eq!(
        receipt.report()["fixture_files"].as_object().unwrap().len(),
        974
    );
    assert_eq!(
        receipt.report()["absent_sources"].as_array().unwrap().len(),
        3
    );
    let source = data.root.join("openpilot/system/hardware/hw.py");
    let original = fs::read(&source).unwrap();
    fs::write(&source, &original).unwrap();
    assert!(
        receipt.verify().is_err(),
        "rewriting identical bytes changes the held source identity"
    );
}

#[test]
fn every_retired_source_reappearance_is_rejected() {
    let data = Admission::prepare();
    data.admit().unwrap();
    for name in RETIRED {
        let target = data.root.join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, b"unexpected retired source").unwrap();
        assert!(data.admit().is_err(), "{name}");
        fs::remove_file(target).unwrap();
    }
}

#[test]
fn altered_expected_bytes_and_additional_members_are_rejected() {
    let data = Admission::prepare();
    let target = data.capture.join("missing-base/source/normalized.json.gz");
    let original = fs::read(&target).unwrap();
    let mut altered = original.clone();
    altered[0] ^= 1;
    fs::write(&target, altered).unwrap();
    assert!(data.admit().is_err());
    fs::write(&target, original).unwrap();
    fs::write(data.capture.join("unadmitted.json"), b"{}").unwrap();
    assert!(data.admit().is_err());
}

#[test]
fn alias_plan_cannot_replace_the_original_plan_identity() {
    let data = Admission::prepare();
    let alternate = data.capture.join("alternate-plan.json.gz");
    fs::copy(data.capture.join("plans.json.gz"), &alternate).unwrap();
    assert!(validate(&data.root, &data.capture, &alternate, true, &data.binary).is_err());
}
