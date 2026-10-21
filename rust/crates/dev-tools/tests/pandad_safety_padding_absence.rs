//! Mandatory interpreter-free execution after retiring the original checker.
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
    assert!(
        matches!(fs::symlink_metadata(repository.join("rust/tools/check_pandad_safety_padding.py")), Err(e) if e.kind() == std::io::ErrorKind::NotFound)
    );
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
    let sanitized = required("PANDAD_PADDING_SOURCE_BINARY");
    let sanitized_manifest = required("PANDAD_PADDING_SOURCE_MANIFEST");
    let source = required("PANDAD_PADDING_TRACE_SOURCE_BINARY");
    let plain_manifest = required("PANDAD_PADDING_TRACE_SOURCE_MANIFEST");
    let sanitized_build: Value =
        serde_json::from_slice(&fs::read(&sanitized_manifest).unwrap()).unwrap();
    let plain_build: Value = serde_json::from_slice(&fs::read(&plain_manifest).unwrap()).unwrap();
    assert_eq!(sanitized_build["sanitize"], true);
    assert_eq!(plain_build["sanitize"], false);
    assert_eq!(sanitized_build["sources"], plain_build["sources"]);
    assert_eq!(plain_build["sources"].as_object().unwrap().len(), 21);
    assert_eq!(
        sanitized_build["binary_sha256"].as_str().unwrap(),
        sha(&fs::read(&sanitized).unwrap())
    );
    assert_eq!(
        plain_build["binary_sha256"].as_str().unwrap(),
        sha(&fs::read(&source).unwrap())
    );
    for (name, digest) in plain_build["sources"].as_object().unwrap() {
        assert_eq!(
            sha(&fs::read(repository.join(name)).unwrap()),
            digest.as_str().unwrap()
        );
    }
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
    let artifact_seals = json!({"plain_traced":&seals,"sanitized_source_sha256":sha(&fs::read(&sanitized).unwrap()),"sanitized_manifest_sha256":sha(&fs::read(&sanitized_manifest).unwrap()),"plain_manifest_sha256":sha(&fs::read(&plain_manifest).unwrap())});
    fs::write(
        proof.join("artifacts.json"),
        serde_json::to_vec_pretty(&artifact_seals).unwrap(),
    )
    .unwrap();
    let sanitized_output = proof.join("sanitized-output");
    let mut sanitized_spec = CommandSpec::new_os(
        std::iter::once(checker.as_os_str().to_owned())
            .chain([
                "--source".into(),
                sanitized.as_os_str().into(),
                "--binary".into(),
                native.as_os_str().into(),
                "--output".into(),
                sanitized_output.as_os_str().into(),
            ])
            .collect(),
        Duration::from_secs(300),
    );
    spec_env(&mut sanitized_spec, &root);
    let sanitized_stdout = proof.join("sanitized.stdout");
    let sanitized_stderr = proof.join("sanitized.stderr");
    let sanitized_status = NativeRunner
        .run_to_files(&sanitized_spec, &sanitized_stdout, &sanitized_stderr)
        .unwrap();
    fs::write(
        proof.join("sanitized-exit.json"),
        serde_json::to_vec_pretty(&json!({"code":sanitized_status.code})).unwrap(),
    )
    .unwrap();
    assert_eq!(
        sanitized_status.code,
        0,
        "{}",
        String::from_utf8_lossy(&fs::read(&sanitized_stderr).unwrap())
    );
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
    spec_env(&mut spec, &root);
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
        serde_json::from_slice::<Value>(&fs::read(&sanitized_stdout).unwrap()).unwrap(),
        original
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(sanitized_output.join("report.json")).unwrap())
            .unwrap(),
        original
    );
    assert_eq!(
        fs::read(&sanitized_stdout).unwrap(),
        fs::read(&stdout).unwrap()
    );
    assert_eq!(
        fs::read(&sanitized_stderr).unwrap(),
        fs::read(&stderr).unwrap()
    );
    assert_eq!(
        fs::read(sanitized_output.join("report.json")).unwrap(),
        fs::read(output.join("report.json")).unwrap()
    );
    let opening = regex::Regex::new(r"\bexecve(?:at)?\(").unwrap();
    let executable =
        regex::Regex::new(r#"\bexecve\(("(?:[^"\\]|\\.)*"), (\[.*\]), .*\) = 0$"#).unwrap();
    let mut attempts = Vec::new();
    for line in fs::read_to_string(&trace).unwrap().lines() {
        assert!(
            !line.contains("execve resumed") && !line.contains("execveat resumed"),
            "resumed executable attempt: {line}"
        );
        if !opening.is_match(line) {
            continue;
        }
        let capture = executable
            .captures(line)
            .unwrap_or_else(|| panic!("failed/opaque/non-execve attempt: {line}"));
        let path: String = serde_json::from_str(&capture[1]).unwrap();
        let argv: Vec<String> = serde_json::from_str(&capture[2]).unwrap();
        assert!(
            seals.contains_key(&path),
            "unsealed executable attempt: {line}"
        );
        attempts.push((path, argv));
    }
    let checker_text = checker.to_str().unwrap().to_owned();
    let source_text = source.to_str().unwrap().to_owned();
    let native_text = native.to_str().unwrap().to_owned();
    let expected_exec = vec![
        (
            checker_text.clone(),
            vec![
                checker_text,
                "--source".into(),
                source_text.clone(),
                "--binary".into(),
                native_text.clone(),
                "--output".into(),
                output.to_str().unwrap().into(),
            ],
        ),
        (source_text.clone(), vec![source_text.clone()]),
        (source_text.clone(), vec![source_text]),
        (native_text.clone(), vec![native_text]),
    ];
    assert_eq!(attempts, expected_exec);
    for (path, digest) in &seals {
        assert_eq!(sha(&fs::read(path).unwrap()), *digest);
    }
    assert_eq!(
        sha(&fs::read(&sanitized).unwrap()),
        sanitized_build["binary_sha256"].as_str().unwrap()
    );
    assert_eq!(
        sha(&fs::read(&sanitized_manifest).unwrap()),
        artifact_seals["sanitized_manifest_sha256"]
            .as_str()
            .unwrap()
    );
    assert_eq!(
        sha(&fs::read(&plain_manifest).unwrap()),
        artifact_seals["plain_manifest_sha256"].as_str().unwrap()
    );
    assert!(
        matches!(fs::symlink_metadata(&retired), Err(e) if e.kind() == std::io::ErrorKind::NotFound)
    );
    fs::write(
        proof.join("native-exec-audit.json"),
        serde_json::to_vec_pretty(&json!({
            "passed": true,
            "historical_checker_absent_in_admission_root": true,
            "repository_python_retired": true,
            "exec_attempts": attempts,
            "sealed_executables_sha256": seals,
            "path": "/nonexistent",
            "sanitized_untraced_comparison": true,
            "report_matches_independent_original": true
        }))
        .unwrap(),
    )
    .unwrap();
}

fn spec_env(spec: &mut CommandSpec, root: &Path) {
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
}
