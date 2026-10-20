//! Fresh interpreter-free execution before retiring the original checker.
#![cfg(all(target_os = "linux", feature = "pandad-safety-padding-native"))]
use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn required(name: &str) -> PathBuf {
    fs::canonicalize(PathBuf::from(
        std::env::var_os(name).unwrap_or_else(|| panic!("required {name}")),
    ))
    .unwrap()
}

#[test]
fn absent_checker_root_executes_only_three_sealed_native_artifacts() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let proof = tempfile::Builder::new()
        .prefix("absence-")
        .tempdir_in(required("PANDAD_PADDING_TEST_OUTPUT"))
        .unwrap()
        .keep();
    let root = proof.join("source-root");
    let contract: Value = serde_json::from_slice(include_bytes!(
        "fixtures/pandad-safety-padding/contract.json"
    ))
    .unwrap();
    let pins = contract["source_sha256"].as_object().unwrap();
    assert_eq!(pins.len(), 6);
    for (name, expected) in pins {
        let bytes = fs::read(repository.join(name)).unwrap();
        assert_eq!(sha(&bytes), expected.as_str().unwrap());
        let path = root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    let retired = root.join("rust/tools/check_pandad_safety_padding.py");
    assert!(root.join("rust/tools").is_dir());
    assert!(
        matches!(fs::symlink_metadata(&retired), Err(e) if e.kind() == std::io::ErrorKind::NotFound)
    );
    let checker = fs::canonicalize(env!("CARGO_BIN_EXE_check-pandad-safety-padding")).unwrap();
    let source = required("PANDAD_PADDING_SOURCE_BINARY");
    let native = required("PANDAD_PADDING_NATIVE_BINARY");
    let seals: BTreeMap<_, _> = [&checker, &source, &native]
        .into_iter()
        .map(|path| {
            (
                path.to_str().unwrap().to_owned(),
                sha(&fs::read(path).unwrap()),
            )
        })
        .collect();
    assert_eq!(seals.len(), 3);
    fs::write(
        proof.join("artifacts.json"),
        serde_json::to_vec_pretty(&seals).unwrap(),
    )
    .unwrap();
    let trace = proof.join("native.exec");
    let output = proof.join("output");
    let mut spec = CommandSpec::new_os(
        vec![
            "/usr/bin/strace".into(),
            "-f".into(),
            "-qq".into(),
            "-s".into(),
            "65535".into(),
            "-e".into(),
            "trace=execve,execveat".into(),
            "-o".into(),
            trace.as_os_str().into(),
            checker.as_os_str().into(),
            "--source".into(),
            source.as_os_str().into(),
            "--binary".into(),
            native.as_os_str().into(),
            "--output".into(),
            output.as_os_str().into(),
        ],
        Duration::from_secs(300),
    );
    spec.set_os_env(BTreeMap::from([
        (OsString::from("PATH"), OsString::from("/nonexistent")),
        (
            OsString::from("OPENPILOT_RUST_ROOT"),
            root.as_os_str().into(),
        ),
    ]));
    spec.env_remove = std::env::vars_os()
        .map(|(key, _)| key)
        .filter(|key| {
            key.as_os_str() != OsStr::new("PATH")
                && key.as_os_str() != OsStr::new("OPENPILOT_RUST_ROOT")
        })
        .collect();
    let stdout = proof.join("native.stdout");
    let stderr = proof.join("native.stderr");
    let observed = NativeRunner.run_to_files(&spec, &stdout, &stderr).unwrap();
    assert_eq!(
        observed.code,
        0,
        "{}",
        String::from_utf8_lossy(&fs::read(&stderr).unwrap())
    );
    let original_bytes =
        include_bytes!("fixtures/pandad-safety-padding/edb42bf0-original/report.json");
    assert_eq!(
        sha(original_bytes),
        "c06b958690b1fe277ffba4ff99f9f0c9f15357ca23b9abf88fe925409c271712"
    );
    let original: Value = serde_json::from_slice(original_bytes).unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(&stdout).unwrap()).unwrap(),
        original
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(output.join("report.json")).unwrap()).unwrap(),
        original
    );
    let opening = regex::Regex::new(r"\bexecve(?:at)?\(").unwrap();
    let executable = regex::Regex::new(r#"\bexecve\(("(?:[^"\\]|\\.)*")"#).unwrap();
    let mut attempts = Vec::new();
    for line in fs::read_to_string(&trace).unwrap().lines() {
        if !opening.is_match(line) {
            continue;
        }
        let capture = executable
            .captures(line)
            .unwrap_or_else(|| panic!("opaque/unaccounted executable attempt: {line}"));
        let path: String = serde_json::from_str(&capture[1]).unwrap();
        assert!(
            seals.contains_key(&path),
            "unsealed executable attempt: {line}"
        );
        attempts.push(path);
    }
    assert_eq!(
        attempts,
        [&checker, &source, &source, &native].map(|p| p.to_str().unwrap().to_owned())
    );
    for (path, digest) in &seals {
        assert_eq!(sha(&fs::read(path).unwrap()), *digest);
    }
    assert!(
        matches!(fs::symlink_metadata(&retired), Err(e) if e.kind() == std::io::ErrorKind::NotFound)
    );
    fs::write(
        proof.join("native-exec-audit.json"),
        serde_json::to_vec_pretty(&json!({
            "passed": true,
            "historical_checker_absent_in_admission_root": true,
            "repository_python_retired": false,
            "exec_attempts": attempts,
            "sealed_executables_sha256": seals,
            "path": "/nonexistent",
            "report_matches_independent_original": true
        }))
        .unwrap(),
    )
    .unwrap();
}
