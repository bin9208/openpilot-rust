#![cfg(feature = "soundd-daemon-native")]

use openpilot_dev_tools::soundd_source_stage::{self, StageOptions};
use serde_json::Value;
use std::{
    fs::{self, hard_link},
    os::unix::fs::symlink,
    path::{Path, PathBuf},
};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap()
}

fn manifest() -> Value {
    serde_json::from_slice(include_bytes!("fixtures/soundd-daemon/manifest.json")).unwrap()
}

fn source_copy() -> tempfile::TempDir {
    let copy = tempfile::tempdir().unwrap();
    for name in manifest()["source_sha256"].as_object().unwrap().keys() {
        let source = repo_root().join(name);
        if name == "rust/tools/check_soundd_daemon.py"
            && fs::symlink_metadata(&source)
                .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
        {
            continue;
        }
        let destination = copy.path().join(name);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::copy(source, destination).unwrap();
    }
    copy
}

fn stage(root: &Path, output: &Path) -> openpilot_dev_tools::seal::Result<Value> {
    soundd_source_stage::run(&StageOptions {
        root: root.to_owned(),
        output: output.to_owned(),
    })
}

fn retained_names() -> Vec<String> {
    manifest()["source_sha256"]
        .as_object()
        .unwrap()
        .keys()
        .filter(|name| name.as_str() != "rust/tools/check_soundd_daemon.py")
        .cloned()
        .collect()
}

#[test]
fn stages_exact_retained_manifest_sources_and_no_receipt_file() {
    let root = source_copy();
    let parent = tempfile::tempdir().unwrap();
    let output = parent.path().join("soundd-source");
    let receipt = stage(root.path(), &output).unwrap();

    let names = retained_names();
    assert_eq!(receipt["status"], "PASS");
    assert_eq!(receipt["copied_files"], 20);
    assert_eq!(receipt["staged_files"].as_array().unwrap().len(), 20);
    assert_eq!(receipt["source_sha256"].as_object().unwrap().len(), 21);
    assert_eq!(
        serde_json::from_value::<Vec<String>>(receipt["staged_files"].clone()).unwrap(),
        names
    );
    assert!(!output.join("rust/tools/check_soundd_daemon.py").exists());
    assert!(!output.join("receipt.json").exists());
    for name in retained_names() {
        assert_eq!(
            fs::read(output.join(&name)).unwrap(),
            fs::read(root.path().join(name)).unwrap()
        );
    }
}

#[test]
fn rejects_missing_or_changed_source_without_publishing_output() {
    for missing in [true, false] {
        let root = source_copy();
        let path = root.path().join("msgq_repo/msgq/__init__.py");
        if missing {
            fs::remove_file(&path).unwrap();
        } else {
            let mut bytes = fs::read(&path).unwrap();
            bytes.push(b'\n');
            fs::write(&path, bytes).unwrap();
        }
        let parent = tempfile::tempdir().unwrap();
        let output = parent.path().join("out");
        assert!(stage(root.path(), &output).is_err());
        assert!(!output.exists());
    }
}

#[test]
fn rejects_symlink_and_hardlink_alias_sources() {
    let first = "msgq_repo/msgq/__init__.py";
    let second = "msgq_repo/msgq/event.cc";
    for hardlink in [false, true] {
        let root = source_copy();
        let a = root.path().join(first);
        let b = root.path().join(second);
        fs::remove_file(&b).unwrap();
        if hardlink {
            hard_link(&a, &b).unwrap();
        } else {
            symlink(&a, &b).unwrap();
        }
        let parent = tempfile::tempdir().unwrap();
        let output = parent.path().join("out");
        assert!(stage(root.path(), &output).is_err());
        assert!(!output.exists());
    }
}

#[test]
fn refuses_existing_output_and_preserves_foreign_contents() {
    let root = source_copy();
    let parent = tempfile::tempdir().unwrap();
    let output = parent.path().join("owned-by-caller");
    fs::create_dir(&output).unwrap();
    fs::write(output.join("sentinel"), b"caller data").unwrap();

    assert!(stage(root.path(), &output).is_err());
    assert_eq!(fs::read(output.join("sentinel")).unwrap(), b"caller data");
    assert_eq!(fs::read_dir(output).unwrap().count(), 1);
}

#[test]
#[ignore = "hosted unchanged Python staging output required; never invokes original locally"]
fn actual_cli_matches_independently_staged_twenty_file_output() {
    use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
    use sha2::{Digest, Sha256};
    use std::time::Duration;
    let required = |name: &str| {
        PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("missing {name}")))
    };
    let root = required("SOUNDD_STAGE_ROOT");
    let original = required("SOUNDD_STAGE_ORIGINAL");
    let output = required("SOUNDD_STAGE_NATIVE");
    let proof = required("SOUNDD_STAGE_COMPARISON");
    let mut spec = CommandSpec::new_os(
        vec![
            env!("CARGO_BIN_EXE_stage-soundd-source").into(),
            "--root".into(),
            root.into_os_string(),
            "--output".into(),
            output.as_os_str().to_owned(),
        ],
        Duration::from_secs(20),
    );
    spec.stdout_limit = 65536;
    spec.stderr_limit = 65536;
    let captured = NativeRunner.run(&spec).unwrap();
    assert_eq!(
        captured.code,
        0,
        "{}",
        String::from_utf8_lossy(&captured.stderr)
    );
    assert!(captured.stderr.is_empty());
    let receipt: Value = serde_json::from_slice(&captured.stdout).unwrap();
    assert_eq!(receipt["status"], "PASS");
    assert_eq!(receipt["copied_files"], 20);
    let pins = manifest();
    let mut comparisons = Vec::new();
    for name in retained_names() {
        let native = fs::read(output.join(&name)).unwrap();
        let expected = fs::read(original.join(&name)).unwrap();
        assert_eq!(native, expected, "original staging differs: {name}");
        let digest = format!("{:x}", Sha256::digest(&native));
        assert_eq!(pins["source_sha256"][&name], digest);
        comparisons.push(serde_json::json!({"name":name,"bytes":native.len(),"sha256":digest}));
    }
    let rejected = NativeRunner.run(&spec).unwrap();
    assert_ne!(rejected.code, 0);
    assert!(rejected.stdout.is_empty());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("output already exists"));
    fs::write(proof, serde_json::to_vec_pretty(&serde_json::json!({"passed":true,"original_output_compared":true,"comparisons":comparisons,"native_receipt":receipt,"existing_output_rejected":true})).unwrap()).unwrap();
}

#[test]
fn rejects_retired_checker_file_and_dangling_symlink_reappearance() {
    for dangling in [false, true] {
        let root = source_copy();
        let retired = root.path().join("rust/tools/check_soundd_daemon.py");
        fs::create_dir_all(retired.parent().unwrap()).unwrap();
        if dangling {
            symlink("missing-checker-body", &retired).unwrap();
        } else {
            fs::write(&retired, b"reappearance control; never executed").unwrap();
        }
        let parent = tempfile::tempdir().unwrap();
        let output = parent.path().join("out");
        assert!(stage(root.path(), &output).is_err());
        assert!(
            fs::symlink_metadata(output).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
        );
        assert!(fs::symlink_metadata(retired).is_ok());
    }
}
