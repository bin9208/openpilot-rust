//! Cloud-only comparison against sealed unchanged-Python requests and C++ responses.
#![cfg(all(target_os = "linux", feature = "pandad-device-native"))]
use base64::{engine::general_purpose::STANDARD, Engine as _};
use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs,
    os::unix::ffi::OsStringExt,
    path::{Path, PathBuf},
    time::Duration,
};

const RETIRED: [&str; 2] = [
    "rust/tools/check_pandad_device.py",
    "rust/tools/pandad_device_cases.py",
];
const ADMISSIONS: [&str; 11] = [
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
fn regular(path: &Path) {
    assert!(
        fs::symlink_metadata(path).unwrap().file_type().is_file(),
        "regular file required: {}",
        path.display()
    );
    for parent in path.ancestors().skip(1) {
        assert!(
            fs::symlink_metadata(parent).unwrap().file_type().is_dir(),
            "symlink/non-directory parent: {}",
            parent.display()
        );
    }
}
fn file_roster(directory: &Path, prefix: &Path, roster: &mut BTreeSet<PathBuf>) {
    assert!(fs::symlink_metadata(directory)
        .unwrap()
        .file_type()
        .is_dir());
    for entry in fs::read_dir(directory).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let kind = fs::symlink_metadata(&path).unwrap().file_type();
        if kind.is_dir() {
            file_roster(&path, prefix, roster);
        } else {
            regular(&path);
            roster.insert(path.strip_prefix(prefix).unwrap().to_owned());
        }
    }
}
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn required(name: &str) -> PathBuf {
    let path = PathBuf::from(
        std::env::var_os(name).unwrap_or_else(|| panic!("mandatory cloud prerequisite: {name}")),
    );
    assert!(
        path.is_absolute() && path.exists(),
        "{name}: {}",
        path.display()
    );
    path
}
fn value(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn hash(path: &Path) -> String {
    regular(path);
    format!("{:x}", Sha256::digest(fs::read(path).unwrap()))
}
fn rows(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn responses(path: &Path) -> Vec<Value> {
    let mut values = rows(path);
    assert_eq!(values.len(), 1746);
    for row in &mut values {
        assert_eq!(row["remaining"], 0);
        for result in row["results"].as_array_mut().unwrap() {
            if let Some(health) = result.get_mut("health") {
                assert!(health
                    .as_object_mut()
                    .unwrap()
                    .remove("interrupt_load")
                    .is_some());
            }
        }
    }
    values
}
fn original() -> (PathBuf, Value) {
    let capture = required("PANDAD_DEVICE_ORIGINAL_CAPTURE");
    let manifest_path = capture.join("capture-manifest.json");
    assert_eq!(
        hash(&manifest_path),
        std::env::var("PANDAD_DEVICE_CAPTURE_MANIFEST_SHA256")
            .expect("mandatory immutable original capture seal")
    );
    let manifest = value(&manifest_path);
    assert_eq!(
        manifest["source_commit"],
        std::env::var("PANDAD_DEVICE_CAPTURE_SOURCE_COMMIT")
            .expect("mandatory original capture source commit")
    );
    assert_eq!(manifest["borrowed_source_run"], 39616860491_u64);
    assert_eq!(manifest["borrowed_source_artifact"], 12352894540_u64);
    assert_eq!(
        manifest["borrowed_zip_sha256"],
        "e3eb33893aef9aed9d21e0d42e4ddcda3a8fdff02a271bedd02823a6c9d70d87"
    );
    assert_eq!(
        manifest["source_binary_sha256"],
        "13b452618b4e467c4475926dce7d37802c7cbad0cfcfc3fb53269936c4496911"
    );
    let contract: Value =
        serde_json::from_str(include_str!("fixtures/pandad-device/contract.json")).unwrap();
    for name in RETIRED {
        assert_eq!(
            manifest["original_source_sha256"][name],
            contract["sources"][name]
        );
    }
    let sources = manifest["source_builder_sources"].as_object().unwrap();
    assert_eq!(sources.len(), 14);
    assert!(sources.contains_key("rust/tools/pandad_device_source.cc"));
    assert!(sources.contains_key("openpilot/selfdrive/pandad/panda.cc"));
    for (name, expected) in sources {
        assert_eq!(
            hash(&root().join(name)),
            expected.as_str().unwrap(),
            "{name}"
        );
    }
    let seals = manifest["files_b64"].as_object().unwrap();
    let mut consumed = vec![
        "positive/input.jsonl".to_owned(),
        "positive/source.jsonl".to_owned(),
        "positive/source.stderr".to_owned(),
        "positive/report.json".to_owned(),
    ];
    consumed.extend(
        ADMISSIONS
            .iter()
            .map(|name| format!("admissions/{name}.json")),
    );
    consumed.extend(
        [
            "admissions/positive.json",
            "admissions/missing-source.json",
            "admissions/missing-native.json",
            "admissions/existing-output.json",
            "missing-source/input.jsonl",
            "missing-native/input.jsonl",
            "missing-native/source.jsonl",
            "missing-native/source.stderr",
            "preexisting-output/report.json",
        ]
        .into_iter()
        .map(str::to_owned),
    );
    for name in consumed {
        assert!(
            seals.contains_key(&STANDARD.encode(name.as_bytes())),
            "unsealed consumed capture file: {name}"
        );
    }
    let mut sealed_roster = BTreeSet::new();
    for (name, seal) in seals {
        let relative = PathBuf::from(OsString::from_vec(STANDARD.decode(name).unwrap()));
        assert!(
            relative
                .components()
                .all(|part| matches!(part, std::path::Component::Normal(_))),
            "unsafe capture path"
        );
        let path = capture.join(&relative);
        assert!(
            sealed_roster.insert(relative.clone()),
            "duplicate decoded capture path"
        );
        assert!(fs::symlink_metadata(&path).unwrap().file_type().is_file());
        assert_eq!(
            fs::metadata(&path).unwrap().len(),
            seal["size"].as_u64().unwrap()
        );
        assert_eq!(hash(&path), seal["sha256"].as_str().unwrap(), "{name}");
    }
    let mut actual_roster = BTreeSet::new();
    file_roster(&capture, &capture, &mut actual_roster);
    assert!(actual_roster.remove(Path::new("capture-manifest.json")));
    assert_eq!(actual_roster, sealed_roster, "complete capture file roster");
    assert_eq!(manifest["cases"].as_object().unwrap().len(), 16);
    assert_eq!(rows(&capture.join("positive/input.jsonl")).len(), 1746);
    assert_eq!(rows(&capture.join("positive/source.jsonl")).len(), 1746);
    assert!(fs::read(capture.join("positive/source.stderr"))
        .unwrap()
        .is_empty());
    (capture, manifest)
}
fn command(root: &Path, args: Vec<OsString>) -> CommandSpec {
    let mut argv = vec![OsString::from(env!("CARGO_BIN_EXE_check-pandad-device"))];
    argv.extend(args);
    let mut command = CommandSpec::new_os(argv, Duration::from_secs(180));
    command.os_env = Some(
        [
            ("OPENPILOT_RUST_ROOT".into(), root.as_os_str().to_owned()),
            ("PATH".into(), "/nonexistent".into()),
        ]
        .into_iter()
        .collect(),
    );
    command.stdout_limit = 1024 * 1024;
    command.stderr_limit = 1024 * 1024;
    command
}
fn checkout_mode() -> String {
    let mode = std::env::var("PANDAD_DEVICE_RETIREMENT_MODE")
        .expect("mandatory explicit predelete/postdelete mode");
    assert!(["predelete", "postdelete"].contains(&mode.as_str()));
    let contract: Value =
        serde_json::from_str(include_str!("fixtures/pandad-device/contract.json")).unwrap();
    for name in RETIRED {
        if mode == "postdelete" {
            assert_eq!(
                fs::symlink_metadata(root().join(name)).unwrap_err().kind(),
                std::io::ErrorKind::NotFound,
                "actual checkout must have retired sources absent"
            );
        } else {
            assert_eq!(hash(&root().join(name)), contract["sources"][name]);
        }
    }
    mode
}
fn private_root(directory: &Path) -> PathBuf {
    let root = directory.join("absent-root");
    let contract: Value =
        serde_json::from_str(include_str!("fixtures/pandad-device/contract.json")).unwrap();
    for (name, expected) in contract["sources"].as_object().unwrap() {
        if RETIRED.contains(&name.as_str()) {
            continue;
        }
        let source = self::root().join(name);
        assert_eq!(hash(&source), expected.as_str().unwrap());
        let target = root.join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(source, target).unwrap();
    }
    root
}
fn source_root_pins(source_root: &Path) {
    let contract: Value =
        serde_json::from_str(include_str!("fixtures/pandad-device/contract.json")).unwrap();
    let absent = RETIRED.map(|name| match fs::symlink_metadata(source_root.join(name)) {
        Ok(_) => false,
        Err(error) => {
            assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
            true
        }
    });
    assert_eq!(absent[0], absent[1]);
    for (name, expected) in contract["sources"].as_object().unwrap() {
        if absent[0] && RETIRED.contains(&name.as_str()) {
            continue;
        }
        assert_eq!(hash(&source_root.join(name)), expected.as_str().unwrap());
    }
}
#[test]
fn sealed_original_requests_and_cpp_responses_match_cold_native_private_absent_and_checkout() {
    let mode = checkout_mode();
    let (capture, manifest) = original();
    let evidence = tempfile::Builder::new()
        .prefix(&format!("device-retirement-{mode}-"))
        .tempdir_in(required("PANDAD_DEVICE_TEST_OUTPUT"))
        .unwrap()
        .keep();
    let source = required("PANDAD_DEVICE_SOURCE_BINARY");
    let native = required("PANDAD_DEVICE_NATIVE_BINARY");
    assert_eq!(hash(&source), manifest["source_binary_sha256"]);
    let before = [hash(&source), hash(&native)];
    let checker = PathBuf::from(env!("CARGO_BIN_EXE_check-pandad-device"));
    let checker_pin = hash(&checker);
    let expected_input = rows(&capture.join("positive/input.jsonl"));
    assert_eq!(
        expected_input
            .iter()
            .map(|row| row["operations"].as_array().unwrap().len())
            .sum::<usize>(),
        2490
    );
    // The expected response lane is exclusively the independently executed C++
    // source output. Captured native.jsonl is never an expected-value provider.
    let expected = responses(&capture.join("positive/source.jsonl"));
    let absent = private_root(&evidence);
    let raw = evidence.join(OsString::from_vec(b"raw-output-\xff".to_vec()));
    assert_eq!(
        manifest["cases"]["raw-output"], 0,
        "independently observed original raw-path admission"
    );
    for (lane, source_root, output) in [
        (
            "private-absent",
            absent.clone(),
            evidence.join("private-absent"),
        ),
        ("checkout", root(), evidence.join("checkout")),
        ("raw-output", absent, raw),
    ] {
        source_root_pins(&source_root);
        let child = NativeRunner
            .run(&command(
                &source_root,
                vec![
                    "--source".into(),
                    source.clone().into_os_string(),
                    "--native".into(),
                    native.clone().into_os_string(),
                    "--output".into(),
                    output.clone().into_os_string(),
                ],
            ))
            .unwrap();
        fs::write(evidence.join(format!("{lane}.stdout")), &child.stdout).unwrap();
        fs::write(evidence.join(format!("{lane}.stderr")), &child.stderr).unwrap();
        assert_eq!(
            child.code,
            0,
            "{lane}: {}",
            String::from_utf8_lossy(&child.stderr)
        );
        assert_eq!(
            rows(&output.join("input.jsonl")),
            expected_input,
            "{lane}: every original request field and case order"
        );
        for name in ["source", "native"] {
            let actual = responses(&output.join(format!("{name}.jsonl")));
            assert_eq!(
                actual, expected,
                "{lane}/{name}: all response fields and case order"
            );
            assert!(fs::read(output.join(format!("{name}.stderr")))
                .unwrap()
                .is_empty());
        }
        let mut report = value(&capture.join("positive/report.json"));
        // The independently captured report binds its own native ELF. This run
        // binds the held current native ELF, which is built separately in CI.
        report["native_sha256"] = Value::String(before[1].clone());
        assert_eq!(value(&output.join("report.json")), report);
        assert_eq!(
            serde_json::from_slice::<Value>(&child.stdout).unwrap(),
            report
        );
        let proof = value(&output.join("provenance.json"));
        let is_absent = RETIRED
            .iter()
            .all(|name| fs::symlink_metadata(source_root.join(name)).is_err());
        assert_eq!(proof["historical_checker_absent"], is_absent);
        assert_eq!(proof["source_executable_sha256"], before[0]);
        assert_eq!(proof["native_runner_executable_sha256"], before[1]);
        assert_eq!([hash(&source), hash(&native)], before);
        assert_eq!(hash(&checker), checker_pin);
        source_root_pins(&source_root);
    }
    assert_eq!(
        original(),
        (capture, manifest),
        "final capture and source pin revalidation"
    );
    assert_eq!(checkout_mode(), mode);
}

#[test]
fn sealed_original_eleven_cli_exit_admissions_match_after_retirement() {
    let mode = checkout_mode();
    let (capture, manifest) = original();
    let private = tempfile::tempdir_in(required("PANDAD_DEVICE_TEST_OUTPUT")).unwrap();
    let checker = PathBuf::from(env!("CARGO_BIN_EXE_check-pandad-device"));
    let checker_pin = hash(&checker);
    for (name, args) in [
        ("missing-required", vec![]),
        ("help", vec!["--help"]),
        ("help-abbrev", vec!["--hel"]),
        ("short-help-attached", vec!["-hfoo"]),
        ("short-help-attached-value", vec!["-hfoo=bar"]),
        ("short-help-cluster", vec!["-hf"]),
        ("short-help-before-attached", vec!["-h", "-hfoo"]),
        ("unknown", vec!["--unknown"]),
        ("source-option-missing", vec!["--source"]),
        (
            "native-required-missing",
            vec!["--source", "s", "--native", "n"],
        ),
        ("runner-consumes-help", vec!["--runner", "--help"]),
    ] {
        let expected = value(&capture.join(format!("admissions/{name}.json")));
        assert_eq!(expected["name"], name);
        let original_args: Vec<Vec<u8>> = expected["argv_b64"]
            .as_array()
            .unwrap()
            .iter()
            .skip(3)
            .map(|arg| STANDARD.decode(arg.as_str().unwrap()).unwrap())
            .collect();
        assert_eq!(
            original_args,
            args.iter()
                .map(|arg| arg.as_bytes().to_vec())
                .collect::<Vec<_>>(),
            "{name}: independent original argv"
        );
        let before = fs::read_dir(private.path()).unwrap().count();
        let mut spec = command(&root(), args.into_iter().map(OsString::from).collect());
        spec.cwd = Some(private.path().to_owned());
        let child = NativeRunner.run(&spec).unwrap();
        assert_eq!(
            i64::from(child.code),
            expected["exit"].as_i64().unwrap(),
            "{name}"
        );
        assert!([0, 2].contains(&child.code));
        let original_stdout = STANDARD
            .decode(expected["stdout_b64"].as_str().unwrap())
            .unwrap();
        let original_stderr = STANDARD
            .decode(expected["stderr_b64"].as_str().unwrap())
            .unwrap();
        assert_eq!(
            child.stdout.is_empty(),
            original_stdout.is_empty(),
            "{name}: stdout channel"
        );
        assert_eq!(
            child.stderr.is_empty(),
            original_stderr.is_empty(),
            "{name}: stderr channel"
        );
        assert!(expected["output_path_b64"].is_null() && expected["output_files"].is_null());
        for bytes in [
            &child.stdout,
            &child.stderr,
            &original_stdout,
            &original_stderr,
        ] {
            assert!(
                !bytes.windows(4).any(|part| part == b"PASS"),
                "{name}: unexpected PASS receipt"
            );
        }
        if child.code == 0 {
            assert!(String::from_utf8_lossy(&child.stdout).contains("--source"));
            assert!(String::from_utf8_lossy(&original_stdout).contains("--source"));
        }
        assert_eq!(
            fs::read_dir(private.path()).unwrap().count(),
            before,
            "{name}: admission created filesystem effects"
        );
        assert_eq!(hash(&checker), checker_pin);
    }
    assert_eq!(
        original(),
        (capture, manifest),
        "final capture and source pin revalidation"
    );
    assert_eq!(checkout_mode(), mode);
}

fn recorded_args(admission: &Value) -> Vec<OsString> {
    admission["argv_b64"]
        .as_array()
        .unwrap()
        .iter()
        .skip(3)
        .map(|arg| OsString::from_vec(STANDARD.decode(arg.as_str().unwrap()).unwrap()))
        .collect()
}

#[test]
fn sealed_original_missing_source_missing_native_and_foreign_output_effects_match() {
    let mode = checkout_mode();
    let (capture, manifest) = original();
    let evidence = tempfile::Builder::new()
        .prefix("device-failures-")
        .tempdir_in(required("PANDAD_DEVICE_TEST_OUTPUT"))
        .unwrap()
        .keep();
    let source = required("PANDAD_DEVICE_SOURCE_BINARY");
    let native = required("PANDAD_DEVICE_NATIVE_BINARY");
    let checker = PathBuf::from(env!("CARGO_BIN_EXE_check-pandad-device"));
    let pins = [hash(&source), hash(&native), hash(&checker)];
    assert_eq!(pins[0], manifest["source_binary_sha256"]);
    let positive = value(&capture.join("admissions/positive.json"));
    let positive_args = recorded_args(&positive);
    assert_eq!(positive_args.len(), 6);
    assert_eq!(positive_args[0], "--source");
    assert_eq!(positive_args[2], "--native");
    assert_eq!(positive_args[4], "--output");
    let historical_capture = Path::new(&positive_args[5]).parent().unwrap();
    let historical_absent = historical_capture.join("absent-probe");
    let absent = evidence.join("absent-probe");
    assert!(!absent.exists());
    for name in ["missing-source", "missing-native", "existing-output"] {
        let receipt = value(&capture.join(format!("admissions/{name}.json")));
        assert_eq!(receipt["name"], name);
        let output_name = if name == "existing-output" {
            "preexisting-output"
        } else {
            name
        };
        let historical_output = historical_capture.join(output_name);
        // Only these explicit source/native/output path roles are substituted.
        // The sealed original argv still establishes order and which probe is absent.
        assert_eq!(
            recorded_args(&receipt),
            vec![
                "--source".into(),
                if name == "missing-source" {
                    historical_absent.clone().into_os_string()
                } else {
                    positive_args[1].clone()
                },
                "--native".into(),
                if name == "missing-native" {
                    historical_absent.clone().into_os_string()
                } else {
                    positive_args[3].clone()
                },
                "--output".into(),
                historical_output.clone().into_os_string(),
            ]
        );
        assert_eq!(
            OsString::from_vec(
                STANDARD
                    .decode(receipt["output_path_b64"].as_str().unwrap())
                    .unwrap()
            ),
            historical_output.as_os_str()
        );
        let output = evidence.join(output_name);
        if name == "existing-output" {
            fs::create_dir(&output).unwrap();
            fs::write(output.join("report.json"), b"foreign receipt").unwrap();
        }
        let child = NativeRunner
            .run(&command(
                &root(),
                vec![
                    "--source".into(),
                    if name == "missing-source" {
                        absent.clone().into_os_string()
                    } else {
                        source.clone().into_os_string()
                    },
                    "--native".into(),
                    if name == "missing-native" {
                        absent.clone().into_os_string()
                    } else {
                        native.clone().into_os_string()
                    },
                    "--output".into(),
                    output.clone().into_os_string(),
                ],
            ))
            .unwrap();
        fs::write(evidence.join(format!("{name}.stdout")), &child.stdout).unwrap();
        fs::write(evidence.join(format!("{name}.stderr")), &child.stderr).unwrap();
        assert_eq!(i64::from(child.code), receipt["exit"].as_i64().unwrap());
        assert_eq!(child.code, 1);
        let original_stdout = STANDARD
            .decode(receipt["stdout_b64"].as_str().unwrap())
            .unwrap();
        let original_stderr = STANDARD
            .decode(receipt["stderr_b64"].as_str().unwrap())
            .unwrap();
        assert!(child.stdout.is_empty() && original_stdout.is_empty());
        assert!(!child.stderr.is_empty() && !original_stderr.is_empty());
        for bytes in [
            &child.stdout,
            &child.stderr,
            &original_stdout,
            &original_stderr,
        ] {
            assert!(
                !bytes.windows(4).any(|part| part == b"PASS"),
                "{name}: false PASS receipt"
            );
        }
        let expected_files = receipt["output_files"].as_object().unwrap();
        let expected_roster: BTreeSet<PathBuf> = match name {
            "missing-source" => ["input.jsonl"].into_iter().map(PathBuf::from).collect(),
            "missing-native" => ["input.jsonl", "source.jsonl", "source.stderr"]
                .into_iter()
                .map(PathBuf::from)
                .collect(),
            _ => ["report.json"].into_iter().map(PathBuf::from).collect(),
        };
        assert_eq!(
            expected_files
                .keys()
                .map(PathBuf::from)
                .collect::<BTreeSet<_>>(),
            expected_roster,
            "independent original failure order"
        );
        let mut actual_files = BTreeSet::new();
        file_roster(&output, &output, &mut actual_files);
        assert_eq!(
            actual_files,
            expected_files
                .keys()
                .map(PathBuf::from)
                .collect::<BTreeSet<_>>(),
            "{name}: source-first failure filesystem ordering"
        );
        for (file, seal) in expected_files {
            let actual = output.join(file);
            let original_file = capture.join(output_name).join(file);
            assert_eq!(hash(&original_file), seal["sha256"].as_str().unwrap());
            assert_eq!(
                fs::metadata(&original_file).unwrap().len(),
                seal["size"].as_u64().unwrap()
            );
            if file == "input.jsonl" {
                // Original Python and Rust JSON whitespace differs; compare the
                // complete independent request fields and ordered rows instead.
                assert_eq!(rows(&actual), rows(&original_file));
            } else {
                assert_eq!(
                    fs::read(&actual).unwrap(),
                    fs::read(&original_file).unwrap(),
                    "{name}/{file}"
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
        assert_eq!([hash(&source), hash(&native), hash(&checker)], pins);
    }
    assert_eq!(
        original(),
        (capture, manifest),
        "final capture and source pin revalidation"
    );
    assert_eq!(checkout_mode(), mode);
}
