#![cfg(target_os = "linux")]

use openpilot_dev_tools::hardware_info_capture::{validate, validate_for_runner};
use serde_json::Value;
use std::{ffi::OsString, fs, os::unix::ffi::OsStringExt, path::PathBuf};

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
fn final_revalidation_rejects_rewritten_or_altered_capture_members() {
    let data = Admission::prepare();
    let receipt = data.admit().unwrap();
    let target = data.capture.join("missing-base/source/normalized.json.gz");
    let original = fs::read(&target).unwrap();
    fs::write(&target, &original).unwrap();
    assert!(
        receipt.verify().is_err(),
        "identical rewrite changes identity"
    );
    let receipt = data.admit().unwrap();
    let mut altered = original;
    altered[0] ^= 1;
    fs::write(&target, altered).unwrap();
    assert!(
        receipt.verify().is_err(),
        "altered compressed bytes rejected"
    );
}

#[test]
fn alias_plan_cannot_replace_the_original_plan_identity() {
    let data = Admission::prepare();
    let alternate = data.capture.join("alternate-plan.json.gz");
    fs::copy(data.capture.join("plans.json.gz"), &alternate).unwrap();
    assert!(validate(&data.root, &data.capture, &alternate, true, &data.binary).is_err());
}

#[test]
fn foreign_target_requires_a_held_current_host_runner() {
    let data = Admission::prepare();
    let runner = data._temporary.path().join("runner-admission-only");
    fs::copy(std::env::current_exe().unwrap(), &runner).unwrap();
    let mut target = fs::read(&data.binary).unwrap();
    let foreign: u16 = if cfg!(target_arch = "x86_64") {
        183
    } else {
        62
    };
    target[18..20].copy_from_slice(&foreign.to_le_bytes());
    fs::write(&data.binary, target).unwrap();
    assert!(data.admit().is_err(), "foreign ELF without a runner");
    let args = [
        runner.clone().into_os_string(),
        OsString::from("-L"),
        OsString::from_vec(b"/sysroot-\xff".to_vec()),
    ];
    let receipt = validate_for_runner(
        &data.root,
        &data.capture,
        &data.capture.join("plans.json.gz"),
        true,
        &data.binary,
        &args,
    )
    .unwrap();
    receipt.verify().unwrap();
    assert_eq!(
        receipt.report()["runner_argv_bytes"][2],
        serde_json::json!(b"/sysroot-\xff".to_vec())
    );
    assert!(validate_for_runner(
        &data.root,
        &data.capture,
        &data.capture.join("plans.json.gz"),
        true,
        &data.binary,
        &[OsString::from("relative-runner")]
    )
    .is_err());
    let original = fs::read(&runner).unwrap();
    fs::write(&runner, original).unwrap();
    assert!(
        receipt.verify().is_err(),
        "runner replacement or rewrite must invalidate receipt"
    );
    // Header admission only: neither the modified target nor the runner is executed.
}
