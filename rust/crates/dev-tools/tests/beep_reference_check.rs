use openpilot_dev_tools::{
    beep_reference_check::{check, Options},
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use sha2::{Digest, Sha256};
use std::{env, fs, path::PathBuf};

fn options(output: PathBuf) -> Options {
    let required = |key| {
        PathBuf::from(env::var_os(key).expect("missing mandatory beep reference cloud input"))
    };
    Options {
        root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."),
        binary: required("BEEP_REFERENCE_BINARY"),
        binding: required("BEEP_REFERENCE_BINDING"),
        original: required("BEEP_REFERENCE_ORIGINAL"),
        original_manifest_sha256: env::var("BEEP_REFERENCE_ORIGINAL_MANIFEST_SHA256")
            .expect("missing mandatory original-only capture hash"),
        output,
    }
}

struct RejectRun(usize);
impl Runner for RejectRun {
    fn run(&mut self, _: &CommandSpec) -> Result<Captured> {
        self.0 += 1;
        Err("beep candidate must not run after failed source admission".into())
    }
}

struct EmptyPeer(usize);
impl Runner for EmptyPeer {
    fn run(&mut self, _: &CommandSpec) -> Result<Captured> {
        self.0 += 1;
        Ok(Captured {
            code: 0,
            stdout: Vec::new(),
            stderr: Vec::new(),
        })
    }
}

#[test]
fn full_original_only_capture_matches_fresh_native_twenty_cases() {
    let output = options(PathBuf::from(
        env::var_os("BEEP_REFERENCE_EVIDENCE").expect("missing beep evidence output"),
    ));
    check(
        &output,
        &mut openpilot_dev_tools::qa_process::NativeRunner,
        &mut std::io::sink(),
    )
    .unwrap();
    let summary: serde_json::Value =
        serde_json::from_slice(&fs::read(output.output.join("summary.json")).unwrap()).unwrap();
    assert_eq!(summary["passed"], true);
    assert_eq!(summary["scenarios"], 20);
}

#[test]
fn changed_original_source_cannot_launch_or_publish() {
    let private = tempfile::tempdir().unwrap();
    let mut options = options(private.path().join("report"));
    let source = options.root.clone();
    options.root = private.path().join("root");
    for name in [
        "rust/tools/check_beepd_reference.py",
        "rust/tools/beep_source.py",
        "rust/tools/beep_fixtures.py",
        "rust/tools/original_params_binding.py",
        "openpilot/selfdrive/controls/beep.py",
        "openpilot/common/utils.py",
        "openpilot/common/realtime.py",
    ] {
        let target = options.root.join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(source.join(name), target).unwrap();
    }
    fs::write(
        options.root.join("rust/tools/beep_source.py"),
        b"changed source",
    )
    .unwrap();
    let mut runner = RejectRun(0);
    assert!(check(&options, &mut runner, &mut std::io::sink()).is_err());
    assert_eq!(runner.0, 0);
    assert!(!options.output.exists());
}

#[test]
fn foreign_capture_manifest_cannot_launch_or_publish() {
    let private = tempfile::tempdir().unwrap();
    let mut options = options(private.path().join("report"));
    options.original_manifest_sha256 = "0".repeat(64);
    let mut runner = RejectRun(0);
    assert!(check(&options, &mut runner, &mut std::io::sink()).is_err());
    assert_eq!(runner.0, 0);
    assert!(!options.output.exists());
}

#[test]
fn self_consistent_binding_with_changed_source_or_json11_cannot_launch() {
    let original: serde_json::Value = serde_json::from_slice(include_bytes!(
        "fixtures/beep-reference/original-binding-provenance-3835653.json"
    ))
    .unwrap();
    for changed in ["sources", "json11_sha256"] {
        let private = tempfile::tempdir().unwrap();
        let mut options = options(private.path().join("report"));
        options.binding = private.path().join("params_pyx_foreign.so");
        let altered = b"freshly rebuilt Params binding with changed source policy";
        fs::write(&options.binding, altered).unwrap();
        let mut provenance = original.clone();
        provenance["module_sha256"] = serde_json::json!(format!("{:x}", Sha256::digest(altered)));
        if changed == "sources" {
            provenance["sources"]["openpilot/common/params.h"] = serde_json::json!("0".repeat(64));
        } else {
            provenance["json11_sha256"] = serde_json::json!("0".repeat(64));
        }
        fs::write(
            private.path().join("provenance.json"),
            serde_json::to_vec(&provenance).unwrap(),
        )
        .unwrap();
        let mut runner = RejectRun(0);
        let error = check(&options, &mut runner, &mut std::io::sink()).unwrap_err();
        let expected_diagnostic = if changed == "sources" {
            "source map differs"
        } else {
            "json11_sha256"
        };
        assert!(
            error.to_string().contains(expected_diagnostic),
            "{changed}: {error}"
        );
        assert_eq!(runner.0, 0);
        assert!(!options.output.exists());
    }
}

#[test]
fn script_candidate_cannot_launch_or_publish() {
    let private = tempfile::tempdir().unwrap();
    let mut options = options(private.path().join("report"));
    options.binary = private.path().join("beep-script");
    fs::write(&options.binary, b"#!/bin/sh\nexit 0\n").unwrap();
    let mut runner = RejectRun(0);
    let error = check(&options, &mut runner, &mut std::io::sink()).unwrap_err();
    assert!(error.to_string().contains("native ELF"), "{error}");
    assert_eq!(runner.0, 0);
    assert!(!options.output.exists());
}

#[test]
fn candidate_missing_policy_trace_cannot_publish_summary() {
    let private = tempfile::tempdir().unwrap();
    let options = options(private.path().join("report"));
    let mut runner = EmptyPeer(0);
    assert!(check(&options, &mut runner, &mut std::io::sink()).is_err());
    assert_eq!(runner.0, 1);
    assert!(!options.output.join("summary.json").exists());
}
