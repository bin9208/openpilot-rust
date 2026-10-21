//! Cloud-only complete original/C++ parity, including raw runner/native paths.
#![cfg(all(target_os = "linux", feature = "pandad-can-io-native"))]
use base64::{engine::general_purpose::STANDARD, Engine as _};
use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
use openpilot_log_format::{JsonValue, JsonView};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs,
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::PermissionsExt,
    },
    path::{Path, PathBuf},
    time::Duration,
};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn required(name: &str) -> PathBuf {
    let p = PathBuf::from(
        std::env::var_os(name).unwrap_or_else(|| panic!("mandatory cloud prerequisite: {name}")),
    );
    assert!(p.is_absolute() && p.exists());
    p
}
fn regular(path: &Path) {
    assert!(fs::symlink_metadata(path).unwrap().is_file());
    for p in path.ancestors().skip(1) {
        assert!(fs::symlink_metadata(p).unwrap().is_dir());
    }
}
fn hash(path: &Path) -> String {
    regular(path);
    format!("{:x}", Sha256::digest(fs::read(path).unwrap()))
}
fn value(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn rows(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}
fn roster(path: &Path, prefix: &Path, files: &mut BTreeSet<PathBuf>) {
    assert!(fs::symlink_metadata(path).unwrap().is_dir());
    for entry in fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if fs::symlink_metadata(&path).unwrap().is_dir() {
            roster(&path, prefix, files);
        } else {
            regular(&path);
            files.insert(path.strip_prefix(prefix).unwrap().into());
        }
    }
}
const RETIRED: [&str; 2] = [
    "rust/tools/check_pandad_can_io.py",
    "rust/tools/pandad_protocol_cases.py",
];
const CLI: [&str; 11] = [
    "missing-required",
    "help",
    "help-abbrev",
    "short-help-attached",
    "short-help-attached-value",
    "short-help-cluster",
    "short-help-before-attached",
    "unknown",
    "source-option-missing",
    "native-required-missing",
    "runner-consumes-help",
];
fn mode() -> String {
    let mode = std::env::var("PANDAD_CAN_IO_RETIREMENT_MODE")
        .expect("explicit predelete/postdelete mode required");
    for (name, pin) in RETIRED.into_iter().zip([
        "416898cf5340e3ff5a9549c11f5e472a147033f888768603d01dced886c8126f",
        "3eac4a39401de9311c5a5ea5211afebac1deede3a055f9d15fdb45d7576ee5bf",
    ]) {
        match mode.as_str() {
            "predelete" => assert_eq!(hash(&root().join(name)), pin),
            "postdelete" => assert_eq!(
                fs::symlink_metadata(root().join(name)).unwrap_err().kind(),
                std::io::ErrorKind::NotFound
            ),
            _ => panic!("unknown CAN-I/O retirement mode"),
        }
    }
    mode
}
fn original() -> (PathBuf, Value) {
    let capture = required("PANDAD_CAN_IO_ORIGINAL_CAPTURE");
    let path = capture.join("capture-manifest.json");
    assert_eq!(
        hash(&path),
        std::env::var("PANDAD_CAN_IO_CAPTURE_MANIFEST_SHA256").unwrap()
    );
    let manifest = value(&path);
    assert_eq!(
        manifest["source_commit"],
        std::env::var("PANDAD_CAN_IO_CAPTURE_SOURCE_COMMIT").unwrap()
    );
    assert_eq!(manifest["borrowed_source_artifact"], 12358686227_u64);
    assert_eq!(
        manifest["borrowed_zip_sha256"],
        "1dffb8ea17d36f68e02b3c59736effbf9667d6225fb13e35eec833f140f321cc"
    );
    assert_eq!(
        manifest["source_binary_sha256"],
        "fadf6c458826866584eb66d5cb19794a36aede895d5bbb3f588d2368fecd9ffc"
    );
    for (name, pin) in RETIRED.into_iter().zip([
        "416898cf5340e3ff5a9549c11f5e472a147033f888768603d01dced886c8126f",
        "3eac4a39401de9311c5a5ea5211afebac1deede3a055f9d15fdb45d7576ee5bf",
    ]) {
        assert_eq!(manifest["original_source_sha256"][name], pin);
    }
    let sources = manifest["source_builder_sources"].as_object().unwrap();
    assert_eq!(sources.len(), 14);
    for (name, pin) in sources {
        assert_eq!(hash(&root().join(name)), pin.as_str().unwrap());
    }
    let mut sealed = BTreeSet::new();
    for (name, pin) in manifest["files_b64"].as_object().unwrap() {
        let rel = PathBuf::from(OsString::from_vec(STANDARD.decode(name).unwrap()));
        assert!(rel
            .components()
            .all(|p| matches!(p, std::path::Component::Normal(_))));
        assert!(sealed.insert(rel.clone()));
        let path = capture.join(rel);
        assert_eq!(hash(&path), pin["sha256"].as_str().unwrap());
        assert_eq!(
            fs::metadata(path).unwrap().len(),
            pin["size"].as_u64().unwrap()
        );
    }
    let mut actual = BTreeSet::new();
    roster(&capture, &capture, &mut actual);
    assert!(actual.remove(Path::new("capture-manifest.json")));
    assert_eq!(actual, sealed);
    for lane in ["positive", "raw-native-output"] {
        for name in ["seeds.input.jsonl", "seeds.jsonl", "report.json"] {
            assert!(sealed.contains(&PathBuf::from(lane).join(name)));
        }
        for name in ["normal", "maxout"] {
            for side in ["source", "native"] {
                for ext in ["input.jsonl", "jsonl", "stderr"] {
                    assert!(sealed.contains(&PathBuf::from(format!("{lane}/{name}/{side}.{ext}"))));
                }
            }
        }
    }
    for name in CLI.into_iter().chain([
        "positive",
        "raw-native",
        "missing-source",
        "missing-native",
        "existing-output",
    ]) {
        assert!(sealed.contains(&PathBuf::from(format!("admissions/{name}.json"))));
    }
    (capture, manifest)
}
fn command(source_root: &Path, args: Vec<OsString>) -> CommandSpec {
    let mut argv = vec![env!("CARGO_BIN_EXE_check-pandad-can-io").into()];
    argv.extend(args);
    let mut spec = CommandSpec::new_os(argv, Duration::from_secs(300));
    spec.os_env = Some(
        [
            (
                "OPENPILOT_RUST_ROOT".into(),
                source_root.as_os_str().to_owned(),
            ),
            ("PATH".into(), "/nonexistent".into()),
            ("PANDAD_MAXOUT".into(), "hostile-inherited-value".into()),
        ]
        .into_iter()
        .collect(),
    );
    spec.stdout_limit = 1024 * 1024;
    spec.stderr_limit = 1024 * 1024;
    spec
}
fn invoke(
    source_root: &Path,
    source: &Path,
    native: &Path,
    output: &Path,
    runner: &[OsString],
) -> CommandSpec {
    let mut args = vec![
        "--source".into(),
        source.into(),
        "--native".into(),
        native.into(),
        "--output".into(),
        output.into(),
    ];
    if !runner.is_empty() {
        args.push("--runner".into());
        args.extend_from_slice(runner);
    }
    command(source_root, args)
}
fn retained(
    evidence: &Path,
    name: &str,
    spec: &CommandSpec,
) -> openpilot_dev_tools::qa_process::Captured {
    let child = NativeRunner.run(spec).unwrap();
    fs::write(evidence.join(format!("{name}.json")),serde_json::to_vec_pretty(&json!({"argv_b64":spec.effective_argv().iter().map(|arg|STANDARD.encode(arg.as_os_str().as_bytes())).collect::<Vec<_>>(),"exit":child.code,"stdout_b64":STANDARD.encode(&child.stdout),"stderr_b64":STANDARD.encode(&child.stderr)})).unwrap()).unwrap();
    child
}
fn report(path: &Path) -> JsonValue {
    JsonValue::parse(&fs::read_to_string(path).unwrap()).unwrap()
}
#[test]
fn full_original_both_modes_all_fields_match_checkout_absent_and_actual_raw_runner_native() {
    let before_mode = mode();
    let (capture, manifest) = original();
    let evidence = tempfile::Builder::new()
        .prefix("can-io-pair-")
        .tempdir_in(required("PANDAD_CAN_IO_TEST_OUTPUT"))
        .unwrap()
        .keep();
    let source = required("PANDAD_CAN_IO_SOURCE_BINARY");
    let native = required("PANDAD_CAN_IO_NATIVE_BINARY");
    let checker = PathBuf::from(env!("CARGO_BIN_EXE_check-pandad-can-io"));
    let pins = [hash(&source), hash(&native), hash(&checker)];
    assert_eq!(pins[0], manifest["source_binary_sha256"]);
    let private = evidence.join("absent-root");
    for name in manifest["source_builder_sources"]
        .as_object()
        .unwrap()
        .keys()
    {
        let target = private.join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(root().join(name), target).unwrap();
    }
    let raw_native = evidence.join(OsString::from_vec(b"native-\xff".to_vec()));
    fs::hard_link(&native, &raw_native).unwrap();
    let raw_runner = evidence.join(OsString::from_vec(b"runner-\xfe".to_vec()));
    fs::copy(capture.join("raw-runner-elf"), &raw_runner).unwrap();
    fs::set_permissions(&raw_runner, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(
        hash(&raw_runner),
        manifest["raw_runner"]["sha256"].as_str().unwrap()
    );
    for (lane, source_root, native_path, runner, original_lane) in [
        ("checkout", root(), native.clone(), vec![], "positive"),
        (
            "private-absent",
            private.clone(),
            native.clone(),
            vec![],
            "positive",
        ),
        (
            "raw",
            private,
            raw_native.clone(),
            vec![raw_runner.clone().into_os_string()],
            "raw-native-output",
        ),
    ] {
        let output = evidence.join(lane);
        let spec = invoke(&source_root, &source, &native_path, &output, &runner);
        let child = retained(&evidence, lane, &spec);
        assert_eq!(child.code, 0, "{}", String::from_utf8_lossy(&child.stderr));
        assert!(child.stderr.is_empty());
        assert_eq!(
            rows(&output.join("seeds.input.jsonl")),
            rows(&capture.join(original_lane).join("seeds.input.jsonl"))
        );
        assert_eq!(
            rows(&output.join("seeds.jsonl")),
            rows(&capture.join(original_lane).join("seeds.jsonl"))
        );
        for (name, count, ops) in [("normal", 1085, 1933), ("maxout", 1082, 1924)] {
            let requests = rows(
                &capture
                    .join(original_lane)
                    .join(name)
                    .join("source.input.jsonl"),
            );
            let expected = rows(&capture.join(original_lane).join(name).join("source.jsonl"));
            assert_eq!(requests.len(), count);
            assert_eq!(
                requests
                    .iter()
                    .map(|r| r["operations"].as_array().unwrap().len())
                    .sum::<usize>(),
                ops
            );
            for side in ["source", "native"] {
                assert_eq!(
                    rows(&output.join(name).join(format!("{side}.input.jsonl"))),
                    requests
                );
                assert_eq!(
                    rows(&output.join(name).join(format!("{side}.jsonl"))),
                    expected
                );
                assert!(fs::read(output.join(name).join(format!("{side}.stderr")))
                    .unwrap()
                    .is_empty());
            }
        }
        let want = report(&capture.join(original_lane).join("report.json"));
        let actual = report(&output.join("report.json"));
        assert_eq!(
            actual.to_json().unwrap(),
            JsonValue::parse(std::str::from_utf8(&child.stdout).unwrap())
                .unwrap()
                .to_json()
                .unwrap()
        );
        let JsonView::Object(fields) = want.view() else {
            panic!("original report object missing")
        };
        let JsonView::Object(actual_fields) = actual.view() else {
            panic!("native report object missing")
        };
        assert_eq!(fields.len(), actual_fields.len());
        for (key, want) in fields {
            let name = key
                .iter()
                .map(|&p| char::from_u32(p).unwrap())
                .collect::<String>();
            let got = actual.get(&name).unwrap();
            if name == "native_command" {
                let JsonView::Array(args) = got.view() else {
                    panic!("complete command missing")
                };
                let mut expected = runner.clone();
                expected.push(native_path.clone().into_os_string());
                assert_eq!(
                    args.iter()
                        .map(|a| openpilot_dev_tools::python_fs::from_json(a).unwrap())
                        .collect::<Vec<_>>(),
                    expected
                );
                if lane == "raw" {
                    let JsonView::Array(original_args) = want.view() else {
                        panic!("original raw command missing")
                    };
                    assert_eq!(original_args.len(), 2);
                    assert_eq!(
                        openpilot_dev_tools::python_fs::from_json(&original_args[0])
                            .unwrap()
                            .as_bytes(),
                        STANDARD
                            .decode(manifest["raw_runner"]["path_b64"].as_str().unwrap())
                            .unwrap()
                    );
                    assert_eq!(
                        openpilot_dev_tools::python_fs::from_json(&original_args[1])
                            .unwrap()
                            .as_bytes(),
                        STANDARD
                            .decode(manifest["raw_native"]["path_b64"].as_str().unwrap())
                            .unwrap()
                    );
                }
            } else if name == "native_sha256" {
                assert_eq!(got.to_utf8().unwrap(), pins[1]);
            } else {
                assert_eq!(got.to_json().unwrap(), want.to_json().unwrap(), "{name}");
            }
        }
        assert_eq!([hash(&source), hash(&native), hash(&checker)], pins);
    }
    fs::rename(raw_native, evidence.join("raw-native-elf")).unwrap();
    fs::rename(raw_runner, evidence.join("raw-runner-elf")).unwrap();
    assert_eq!(original(), (capture, manifest));
    assert_eq!(mode(), before_mode);
}
#[test]
fn original_cli_admissions_meaning_channels_and_no_effects_match() {
    let before_mode = mode();
    let (capture, manifest) = original();
    let evidence = tempfile::Builder::new()
        .prefix("can-io-cli-")
        .tempdir_in(required("PANDAD_CAN_IO_TEST_OUTPUT"))
        .unwrap()
        .keep();
    let cwd = tempfile::tempdir().unwrap();
    for name in CLI {
        let receipt = value(&capture.join(format!("admissions/{name}.json")));
        let argv = receipt["argv_b64"]
            .as_array()
            .unwrap()
            .iter()
            .skip(3)
            .map(|v| OsString::from_vec(STANDARD.decode(v.as_str().unwrap()).unwrap()))
            .collect();
        let mut spec = command(&root(), argv);
        spec.cwd = Some(cwd.path().into());
        let child = retained(&evidence, name, &spec);
        assert_eq!(i64::from(child.code), receipt["exit"].as_i64().unwrap());
        let stdout = STANDARD
            .decode(receipt["stdout_b64"].as_str().unwrap())
            .unwrap();
        let stderr = STANDARD
            .decode(receipt["stderr_b64"].as_str().unwrap())
            .unwrap();
        assert_eq!(child.stdout.is_empty(), stdout.is_empty());
        assert_eq!(child.stderr.is_empty(), stderr.is_empty());
        for bytes in [&child.stdout, &child.stderr, &stdout, &stderr] {
            assert!(!bytes.windows(4).any(|b| b == b"PASS"));
        }
        if child.code == 0 {
            for flag in ["--source", "--native", "--output", "--runner"] {
                assert!(String::from_utf8_lossy(&child.stdout).contains(flag));
                assert!(String::from_utf8_lossy(&stdout).contains(flag));
            }
        } else {
            let actual = String::from_utf8_lossy(&child.stderr);
            let original = String::from_utf8_lossy(&stderr);
            match name {
                "unknown" => {
                    assert!(actual.contains("required") && original.contains("required"));
                    assert!(actual.contains("--source") && original.contains("--source"));
                }
                "source-option-missing" => {
                    assert!(actual.contains("--source") && actual.contains("value"));
                    assert!(original.contains("--source") && original.contains("argument"));
                }
                _ => assert!(actual.contains("required") && original.contains("required")),
            }
        }
        assert_eq!(fs::read_dir(cwd.path()).unwrap().count(), 0);
    }
    assert_eq!(original(), (capture, manifest));
    assert_eq!(mode(), before_mode);
}

#[test]
fn independently_captured_runner_separator_attached_values_and_help_order_match() {
    let before_mode = mode();
    let (capture, manifest) = original();
    let evidence = tempfile::Builder::new()
        .prefix("can-io-runner-admissions-")
        .tempdir_in(required("PANDAD_CAN_IO_TEST_OUTPUT"))
        .unwrap()
        .keep();
    let old = value(&capture.join("admissions/runner-separator-rejected.json"));
    let old_args = old["argv_b64"]
        .as_array()
        .unwrap()
        .iter()
        .skip(3)
        .map(|v| OsString::from_vec(STANDARD.decode(v.as_str().unwrap()).unwrap()))
        .collect::<Vec<_>>();
    assert_eq!(old_args.len(), 9);
    assert_eq!(old_args[0], "--source");
    assert_eq!(old_args[2], "--native");
    assert_eq!(old_args[4], "--output");
    assert_eq!(old_args[6], "--runner");
    assert_eq!(old_args[8], "--");
    assert_eq!(old["exit"], 2);
    assert!(old["files_b64"].is_null());
    assert!(STANDARD
        .decode(old["stdout_b64"].as_str().unwrap())
        .unwrap()
        .is_empty());
    assert!(String::from_utf8_lossy(
        &STANDARD
            .decode(old["stderr_b64"].as_str().unwrap())
            .unwrap()
    )
    .contains("unrecognized arguments: --"));
    let source = required("PANDAD_CAN_IO_SOURCE_BINARY");
    let native = required("PANDAD_CAN_IO_NATIVE_BINARY");
    let rejected_output = evidence.join("separator-output");
    let child = retained(
        &evidence,
        "actual-captured-separator",
        &invoke(
            &root(),
            &source,
            &native,
            &rejected_output,
            &["/usr/bin/env".into(), "--".into()],
        ),
    );
    assert_eq!(child.code, 2);
    assert!(child.stdout.is_empty());
    assert!(String::from_utf8_lossy(&child.stderr).contains("unrecognized arguments: --"));
    assert!(!rejected_output.exists());
    let probes = manifest["parser_probes"]
        .as_array()
        .expect("fresh original attached/bare parser admissions are mandatory");
    assert_eq!(probes.len(), 12);
    for probe in probes {
        let name = probe.as_str().unwrap();
        let receipt = value(&capture.join(format!("admissions/parser-{name}.json")));
        assert_eq!(receipt["name"], name);
        let mut args = receipt["argv_b64"]
            .as_array()
            .unwrap()
            .iter()
            .skip(3)
            .map(|v| OsString::from_vec(STANDARD.decode(v.as_str().unwrap()).unwrap()))
            .collect::<Vec<_>>();
        assert!(args.len() >= 6);
        assert_eq!(args[0], "--source");
        assert_eq!(args[2], "--native");
        assert_eq!(args[4], "--output");
        let directory = evidence.join(name);
        fs::create_dir(&directory).unwrap();
        let output = directory.join("output");
        // Only the three already-classified source/native/output roles change.
        // All attached/bare/separator/help tokens replay byte-for-byte.
        args[1] = evidence.join("absent-source").into_os_string();
        args[3] = evidence.join("absent-native").into_os_string();
        args[5] = output.clone().into_os_string();
        let child = retained(
            &evidence,
            &format!("parser-{name}"),
            &command(&root(), args),
        );
        assert_eq!(
            i64::from(child.code),
            receipt["exit"].as_i64().unwrap(),
            "{name}: independent original exit"
        );
        let stdout = STANDARD
            .decode(receipt["stdout_b64"].as_str().unwrap())
            .unwrap();
        let stderr = STANDARD
            .decode(receipt["stderr_b64"].as_str().unwrap())
            .unwrap();
        assert_eq!(
            child.stdout.is_empty(),
            stdout.is_empty(),
            "{name}: stdout channel"
        );
        assert_eq!(
            child.stderr.is_empty(),
            stderr.is_empty(),
            "{name}: stderr channel"
        );
        for bytes in [&child.stdout, &child.stderr, &stdout, &stderr] {
            assert!(!bytes.windows(4).any(|part| part == b"PASS"));
        }
        match child.code {
            0 => {
                assert!(receipt["files_b64"].is_null());
                for flag in ["--source", "--native", "--output", "--runner"] {
                    assert!(String::from_utf8_lossy(&child.stdout).contains(flag));
                    assert!(String::from_utf8_lossy(&stdout).contains(flag));
                }
            }
            2 => {
                assert!(receipt["files_b64"].is_null());
                assert!(String::from_utf8_lossy(&child.stderr).contains("unrecognized"));
                assert!(String::from_utf8_lossy(&stderr).contains("unrecognized"));
            }
            1 => {
                let seals = receipt["files_b64"].as_object().unwrap();
                assert_eq!(seals.len(), 1);
                assert!(seals.contains_key(&STANDARD.encode(b"seeds.input.jsonl")));
                let mut files = BTreeSet::new();
                roster(&output, &output, &mut files);
                assert_eq!(
                    files,
                    [PathBuf::from("seeds.input.jsonl")]
                        .into_iter()
                        .collect::<BTreeSet<_>>()
                );
                assert_eq!(
                    rows(&output.join("seeds.input.jsonl")),
                    rows(
                        &capture
                            .join("parser-effects")
                            .join(name)
                            .join("seeds.input.jsonl")
                    )
                );
                assert!(String::from_utf8_lossy(&child.stderr).contains("No such file"));
                assert!(String::from_utf8_lossy(&stderr).contains("No such file"));
            }
            code => panic!("unexpected original parser status {code}"),
        }
        if child.code != 1 {
            assert_eq!(fs::read_dir(&directory).unwrap().count(), 0);
        }
        assert!(!output.join("report.json").exists());
        assert!(!output.join("provenance.json").exists());
    }
    assert_eq!(original(), (capture, manifest));
    assert_eq!(mode(), before_mode);
}
#[test]
fn original_missing_probe_order_and_foreign_receipts_match() {
    let before_mode = mode();
    let (capture, manifest) = original();
    let evidence = tempfile::Builder::new()
        .prefix("can-io-failures-")
        .tempdir_in(required("PANDAD_CAN_IO_TEST_OUTPUT"))
        .unwrap()
        .keep();
    let source = required("PANDAD_CAN_IO_SOURCE_BINARY");
    let native = required("PANDAD_CAN_IO_NATIVE_BINARY");
    let absent = evidence.join("absent-probe");
    for name in ["missing-source", "missing-native", "existing-output"] {
        let receipt = value(&capture.join(format!("admissions/{name}.json")));
        let directory = if name == "existing-output" {
            "preexisting-output"
        } else {
            name
        };
        let output = evidence.join(directory);
        if name == "existing-output" {
            fs::create_dir(&output).unwrap();
            fs::write(output.join("report.json"), b"foreign receipt").unwrap();
        }
        let child = retained(
            &evidence,
            name,
            &invoke(
                &root(),
                if name == "missing-source" {
                    &absent
                } else {
                    &source
                },
                if name == "missing-native" {
                    &absent
                } else {
                    &native
                },
                &output,
                &[],
            ),
        );
        assert_eq!(i64::from(child.code), receipt["exit"].as_i64().unwrap());
        assert_eq!(child.code, 1);
        assert!(child.stdout.is_empty());
        assert!(!child.stderr.is_empty());
        let seals = receipt["files_b64"].as_object().unwrap();
        let expected = seals
            .keys()
            .map(|s| PathBuf::from(OsString::from_vec(STANDARD.decode(s).unwrap())))
            .collect::<BTreeSet<_>>();
        let mut actual = BTreeSet::new();
        roster(&output, &output, &mut actual);
        assert_eq!(actual, expected);
        for relative in expected {
            let original_file = capture.join(directory).join(&relative);
            let seal = seals
                .get(&STANDARD.encode(relative.as_os_str().as_bytes()))
                .unwrap();
            assert_eq!(hash(&original_file), seal["sha256"].as_str().unwrap());
            if relative.extension().is_some_and(|ext| ext == "jsonl") {
                assert_eq!(rows(&output.join(relative)), rows(&original_file));
            } else {
                assert_eq!(
                    fs::read(output.join(relative)).unwrap(),
                    fs::read(original_file).unwrap()
                );
            }
        }
        assert!(!output.join("provenance.json").exists());
        if name == "existing-output" {
            assert_eq!(
                fs::read(output.join("report.json")).unwrap(),
                b"foreign receipt"
            );
        } else {
            assert!(!output.join("report.json").exists());
        }
    }
    assert_eq!(original(), (capture, manifest));
    assert_eq!(mode(), before_mode);
}
