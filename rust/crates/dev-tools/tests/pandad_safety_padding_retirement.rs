//! One fresh original/present/absent proof with identical probe executables.
#![cfg(all(target_os = "linux", feature = "pandad-safety-padding-native"))]
use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs, io,
    path::{Path, PathBuf},
    time::Duration,
};

const HISTORICAL: &str = "rust/tools/check_pandad_safety_padding.py";
fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn required(name: &str) -> PathBuf {
    fs::canonicalize(
        std::env::var_os(name).unwrap_or_else(|| panic!("mandatory cloud prerequisite: {name}")),
    )
    .unwrap()
}
fn sha(path: &Path) -> String {
    format!("{:x}", Sha256::digest(fs::read(path).unwrap()))
}
fn json_file(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn missing(path: &Path) {
    assert!(matches!(fs::symlink_metadata(path), Err(e) if e.kind() == io::ErrorKind::NotFound));
}
fn native_env(spec: &mut CommandSpec, root: &Path) {
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
fn args(source: &Path, binary: &Path, output: &Path) -> Vec<OsString> {
    vec![
        "--source".into(),
        source.as_os_str().into(),
        "--binary".into(),
        binary.as_os_str().into(),
        "--output".into(),
        output.as_os_str().into(),
    ]
}
fn traced_args(
    trace: &Path,
    checker: &Path,
    source: &Path,
    binary: &Path,
    output: &Path,
) -> Vec<OsString> {
    let mut argv = vec![
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
    ];
    argv.extend(args(source, binary, output));
    argv
}
fn rows(path: &Path, parent: &Path, label: &str) -> Vec<Value> {
    let mut result = fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(result.len(), if label == "car-params" { 1 } else { 4 });
    if label != "car-params" {
        let child = if label == "source" {
            "source-params"
        } else {
            "native-params"
        };
        for row in &mut result {
            let owned = row.as_object_mut().unwrap().remove("params_root").unwrap();
            assert_eq!(owned.as_str(), parent.join(child).to_str());
        }
    }
    result
}
fn exec_attempts(trace: &Path, sealed: &BTreeMap<String, String>) -> Vec<(String, Vec<String>)> {
    let opening = regex::Regex::new(r"\bexecve(?:at)?\(").unwrap();
    let invocation =
        regex::Regex::new(r#"\bexecve\(("(?:[^"\\]|\\.)*"), (\[.*\]), .*\) = 0$"#).unwrap();
    let mut attempts = Vec::new();
    for line in fs::read_to_string(trace).unwrap().lines() {
        assert!(
            !line.contains("execve resumed") && !line.contains("execveat resumed"),
            "resumed executable attempt: {line}"
        );
        if !opening.is_match(line) {
            continue;
        }
        let capture = invocation
            .captures(line)
            .unwrap_or_else(|| panic!("failed, opaque or non-execve attempt: {line}"));
        let path: String = serde_json::from_str(&capture[1]).unwrap();
        let argv: Vec<String> = serde_json::from_str(&capture[2]).unwrap();
        assert!(
            sealed.contains_key(&path),
            "unsealed executable attempt: {line}"
        );
        attempts.push((path, argv));
    }
    attempts
}

#[test]
fn fresh_original_and_same_elf_present_absent_are_equivalent_and_interpreter_free() {
    let proof = tempfile::Builder::new()
        .prefix("retirement-")
        .tempdir_in(required("PANDAD_PADDING_TEST_OUTPUT"))
        .unwrap()
        .keep();
    let source = required("PANDAD_PADDING_SOURCE_BINARY");
    let plain_source = required("PANDAD_PADDING_TRACE_SOURCE_BINARY");
    let binary = required("PANDAD_PADDING_NATIVE_BINARY");
    let manifest = required("PANDAD_PADDING_SOURCE_MANIFEST");
    let plain_manifest = required("PANDAD_PADDING_TRACE_SOURCE_MANIFEST");
    let python = required("PANDAD_PADDING_PYTHON");
    let python_sha = sha(&python);
    let checker = fs::canonicalize(env!("CARGO_BIN_EXE_check-pandad-safety-padding")).unwrap();
    let repo = repository();
    let contract: Value = serde_json::from_slice(include_bytes!(
        "fixtures/pandad-safety-padding/contract.json"
    ))
    .unwrap();
    let historical = repo.join(HISTORICAL);
    assert_eq!(
        sha(&historical),
        contract["retired_checker"]["pre_retirement_sha256"]
            .as_str()
            .unwrap()
    );
    let pins = contract["source_sha256"].as_object().unwrap();
    assert_eq!(pins.len(), 6);
    let absent_root = proof.join("absent-root");
    for (name, digest) in pins {
        let path = repo.join(name);
        assert_eq!(sha(&path), digest.as_str().unwrap(), "source pin: {name}");
        let target = absent_root.join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(path, &target).unwrap();
        assert_eq!(sha(&target), digest.as_str().unwrap());
    }
    missing(&absent_root.join(HISTORICAL));
    let manifest_sha = sha(&manifest);
    let plain_manifest_sha = sha(&plain_manifest);
    let build = json_file(&manifest);
    let plain_build = json_file(&plain_manifest);
    assert_eq!(build["sanitize"], true);
    assert_eq!(plain_build["sanitize"], false);
    assert_eq!(plain_build["sources"], build["sources"]);
    let captured_bytes =
        include_bytes!("fixtures/pandad-safety-padding/edb42bf0-source-build-manifest.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(captured_bytes)),
        "5590942fb8df47efe83cd9d34742439e195e1674a2180bca74cdd291c2e6dbb7"
    );
    let captured: Value = serde_json::from_slice(captured_bytes).unwrap();
    assert_eq!(build["sources"], captured["sources"]);
    assert_eq!(build["sources"].as_object().unwrap().len(), 21);
    for (name, digest) in build["sources"].as_object().unwrap() {
        assert_eq!(
            sha(&repo.join(name)),
            digest.as_str().unwrap(),
            "build pin: {name}"
        );
    }
    let sealed: BTreeMap<String, String> = [&checker, &source, &binary]
        .into_iter()
        .map(|path| (path.to_str().unwrap().to_owned(), sha(path)))
        .collect();
    assert_eq!(sealed.len(), 3);
    assert_eq!(
        build["binary_sha256"].as_str().unwrap(),
        sealed[source.to_str().unwrap()]
    );
    assert_eq!(Path::new(build["binary"].as_str().unwrap()), source);
    assert_eq!(
        plain_build["binary_sha256"].as_str().unwrap(),
        sha(&plain_source)
    );
    assert_eq!(
        Path::new(plain_build["binary"].as_str().unwrap()),
        plain_source
    );
    let plain_sealed: BTreeMap<String, String> = [&checker, &plain_source, &binary]
        .into_iter()
        .map(|path| (path.to_str().unwrap().to_owned(), sha(path)))
        .collect();
    assert_eq!(plain_sealed.len(), 3);
    fs::write(
        proof.join("executables.before.json"),
        serde_json::to_vec_pretty(
            &json!({"sanitized_native_lane":&sealed,"plain_trace_lane":&plain_sealed,"sanitized_manifest_sha256":&manifest_sha,"plain_manifest_sha256":&plain_manifest_sha,"original_python_sha256":&python_sha}),
        )
        .unwrap(),
    )
    .unwrap();

    let original = proof.join("original");
    let present = proof.join("native-present");
    let absent = proof.join("native-absent");
    let mut original_argv = vec![python.clone().into(), "-P".into(), historical.into()];
    original_argv.extend(args(&source, &binary, &original));
    let mut original_spec = CommandSpec::new_os(original_argv, Duration::from_secs(300));
    original_spec
        .env
        .insert("OPENPILOT_RUST_ROOT".into(), repo.to_str().unwrap().into());
    original_spec.env.insert(
        "PYTHONPATH".into(),
        std::env::join_paths([&repo, &repo.join("rust/tools")])
            .unwrap()
            .to_str()
            .unwrap()
            .into(),
    );
    let original_status = NativeRunner
        .run_to_files(
            &original_spec,
            &proof.join("original.stdout"),
            &proof.join("original.stderr"),
        )
        .unwrap();
    assert_eq!(original_status.code, 0, "sanitized original source failed");

    // Complete the sanitizer comparisons before any process is placed under ptrace.
    // LeakSanitizer cannot run under strace, so the later audit uses a separately
    // built plain copy of these exact 21 original source files.
    let mut present_sanitized_spec = CommandSpec::new_os(
        std::iter::once(checker.as_os_str().to_owned())
            .chain(args(&source, &binary, &present))
            .collect(),
        Duration::from_secs(300),
    );
    native_env(&mut present_sanitized_spec, &repo);
    let present_sanitized_status = NativeRunner
        .run_to_files(
            &present_sanitized_spec,
            &proof.join("native-present-sanitized.stdout"),
            &proof.join("native-present-sanitized.stderr"),
        )
        .unwrap();
    assert_eq!(present_sanitized_status.code, 0, "sanitized present failed");
    let mut absent_sanitized_spec = CommandSpec::new_os(
        std::iter::once(checker.as_os_str().to_owned())
            .chain(args(&source, &binary, &absent))
            .collect(),
        Duration::from_secs(300),
    );
    native_env(&mut absent_sanitized_spec, &absent_root);
    let absent_sanitized_status = NativeRunner
        .run_to_files(
            &absent_sanitized_spec,
            &proof.join("native-absent-sanitized.stdout"),
            &proof.join("native-absent-sanitized.stderr"),
        )
        .unwrap();
    assert_eq!(absent_sanitized_status.code, 0, "sanitized absent failed");
    let expected = json_file(&original.join("report.json"));
    for (lane, output) in [("present", &present), ("absent", &absent)] {
        assert_eq!(
            json_file(&output.join("report.json")),
            expected,
            "{lane} sanitized report"
        );
        for label in ["car-params", "source", "native"] {
            for suffix in ["stdout", "stderr"] {
                let name = format!("{label}.{suffix}");
                assert_eq!(
                    fs::read(original.join(&name)).unwrap(),
                    fs::read(output.join(&name)).unwrap(),
                    "{lane} sanitized: {name}"
                );
            }
            let name = format!("{label}.input.jsonl");
            assert_eq!(
                rows(&original.join(&name), &original, label),
                rows(&output.join(&name), output, label),
                "{lane} sanitized: {name}"
            );
        }
    }
    fs::write(proof.join("sanitized-gate.json"), serde_json::to_vec_pretty(&json!({"passed":true,"original":original_status.code,"present":present_sanitized_status.code,"absent":absent_sanitized_status.code,"source_sha256":sealed[source.to_str().unwrap()]})).unwrap()).unwrap();

    let plain_present = proof.join("native-present-plain");
    let plain_absent = proof.join("native-absent-plain");
    let present_trace = proof.join("native-present.exec");
    let mut present_spec = CommandSpec::new_os(
        traced_args(
            &present_trace,
            &checker,
            &plain_source,
            &binary,
            &plain_present,
        ),
        Duration::from_secs(300),
    );
    native_env(&mut present_spec, &repo);
    let present_status = NativeRunner
        .run_to_files(
            &present_spec,
            &proof.join("native-present.stdout"),
            &proof.join("native-present.stderr"),
        )
        .unwrap();

    let absent_trace = proof.join("native-absent.exec");
    let mut absent_spec = CommandSpec::new_os(
        traced_args(
            &absent_trace,
            &checker,
            &plain_source,
            &binary,
            &plain_absent,
        ),
        Duration::from_secs(300),
    );
    native_env(&mut absent_spec, &absent_root);
    let absent_status = NativeRunner
        .run_to_files(
            &absent_spec,
            &proof.join("native-absent.stdout"),
            &proof.join("native-absent.stderr"),
        )
        .unwrap();
    fs::write(
        proof.join("exit-codes.json"),
        serde_json::to_vec_pretty(&json!({"original":original_status.code,"present":present_status.code,"absent":absent_status.code})).unwrap(),
    ).unwrap();
    assert_eq!(
        [
            original_status.code,
            present_status.code,
            absent_status.code
        ],
        [0, 0, 0]
    );
    assert_eq!(
        fs::read(proof.join("native-present.stdout")).unwrap(),
        fs::read(proof.join("native-absent.stdout")).unwrap()
    );
    assert_eq!(
        fs::read(proof.join("native-present.stderr")).unwrap(),
        fs::read(proof.join("native-absent.stderr")).unwrap()
    );
    assert_eq!(
        fs::read(proof.join("original.stderr")).unwrap(),
        fs::read(proof.join("native-present.stderr")).unwrap()
    );

    let sealed_original =
        include_bytes!("fixtures/pandad-safety-padding/edb42bf0-original/report.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(sealed_original)),
        "c06b958690b1fe277ffba4ff99f9f0c9f15357ca23b9abf88fe925409c271712"
    );
    assert_eq!(
        expected,
        serde_json::from_slice::<Value>(sealed_original).unwrap()
    );
    assert_eq!(json_file(&proof.join("original.stdout")), expected);
    for (lane, output, has_checker) in [("present", &present, true), ("absent", &absent, false)] {
        assert_eq!(
            json_file(&output.join("report.json")),
            expected,
            "{lane} report"
        );
        assert_eq!(
            json_file(&proof.join(format!("native-{lane}.stdout"))),
            expected
        );
        let admission = json_file(&output.join("provenance.json"));
        assert_eq!(
            admission["executables"]["source"].as_str().unwrap(),
            sealed[source.to_str().unwrap()].as_str()
        );
        assert_eq!(
            admission["executables"]["native"].as_str().unwrap(),
            sealed[binary.to_str().unwrap()].as_str()
        );
        let admitted = admission["admitted_source_sha256"].as_object().unwrap();
        assert_eq!(admitted.len(), 6 + usize::from(has_checker));
        for (name, digest) in pins {
            assert_eq!(admitted.get(name).unwrap(), digest, "{lane}: {name}");
        }
        assert_eq!(admitted.get(HISTORICAL).is_some(), has_checker);
        for label in ["car-params", "source", "native"] {
            for suffix in ["stdout", "stderr"] {
                let name = format!("{label}.{suffix}");
                assert_eq!(
                    fs::read(original.join(&name)).unwrap(),
                    fs::read(output.join(&name)).unwrap(),
                    "{lane}: {name}"
                );
            }
            let name = format!("{label}.input.jsonl");
            assert_eq!(
                rows(&original.join(&name), &original, label),
                rows(&output.join(&name), output, label),
                "{lane}: {name}"
            );
        }
    }
    for (lane, output) in [("present", &plain_present), ("absent", &plain_absent)] {
        assert_eq!(
            json_file(&output.join("report.json")),
            expected,
            "{lane} plain report"
        );
        for label in ["car-params", "source", "native"] {
            for suffix in ["stdout", "stderr"] {
                let name = format!("{label}.{suffix}");
                assert_eq!(
                    fs::read(original.join(&name)).unwrap(),
                    fs::read(output.join(&name)).unwrap(),
                    "{lane} plain: {name}"
                );
            }
            let name = format!("{label}.input.jsonl");
            assert_eq!(
                rows(&original.join(&name), &original, label),
                rows(&output.join(&name), output, label),
                "{lane} plain: {name}"
            );
        }
    }
    let as_text = |path: &Path| path.to_str().unwrap().to_owned();
    let checker_path = as_text(&checker);
    let source_path = as_text(&plain_source);
    let binary_path = as_text(&binary);
    for (lane, trace, output) in [
        ("present", &present_trace, &plain_present),
        ("absent", &absent_trace, &plain_absent),
    ] {
        let expected_exec = vec![
            (
                checker_path.clone(),
                vec![
                    checker_path.clone(),
                    "--source".into(),
                    source_path.clone(),
                    "--binary".into(),
                    binary_path.clone(),
                    "--output".into(),
                    as_text(output),
                ],
            ),
            (source_path.clone(), vec![source_path.clone()]),
            (source_path.clone(), vec![source_path.clone()]),
            (binary_path.clone(), vec![binary_path.clone()]),
        ];
        let attempts = exec_attempts(trace, &plain_sealed);
        fs::write(
            proof.join(format!("native-{lane}-exec-audit.json")),
            serde_json::to_vec_pretty(
                &json!({"measured_exec_attempts_with_argv":&attempts,"expected_exec_attempts_with_argv":&expected_exec,"sealed_executables_sha256":&plain_sealed}),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(attempts, expected_exec, "{lane} unexpected executable/argv");
    }
    for (path, digest) in &sealed {
        assert_eq!(sha(Path::new(path)), *digest, "changed executable: {path}");
    }
    for (path, digest) in &plain_sealed {
        assert_eq!(sha(Path::new(path)), *digest, "changed executable: {path}");
    }
    assert_eq!(sha(&python), python_sha);
    for (name, digest) in pins {
        assert_eq!(sha(&repo.join(name)), digest.as_str().unwrap());
        assert_eq!(sha(&absent_root.join(name)), digest.as_str().unwrap());
    }
    for (name, digest) in build["sources"].as_object().unwrap() {
        assert_eq!(sha(&repo.join(name)), digest.as_str().unwrap());
    }
    assert_eq!(sha(&manifest), manifest_sha);
    assert_eq!(sha(&plain_manifest), plain_manifest_sha);
    assert_eq!(
        sha(&repo.join(HISTORICAL)),
        contract["retired_checker"]["pre_retirement_sha256"]
            .as_str()
            .unwrap()
    );
    missing(&absent_root.join(HISTORICAL));
    fs::write(proof.join("comparison.json"), serde_json::to_vec_pretty(&json!({"pass":true,"four_cases":true,"sanitized_original_present_absent_untraced":true,"plain_trace_source_same_21_inputs":true,"native_probe_elf_same_in_all_lanes":true,"interpreter_free_absent_lane":true})).unwrap()).unwrap();
}
