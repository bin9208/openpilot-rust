//! Hosted full comparison with independently sealed unchanged Python/C++ outputs.
#![cfg(all(target_os = "linux", feature = "pandad-protocol-native"))]
use base64::{engine::general_purpose::STANDARD, Engine as _};
use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs,
    os::unix::ffi::OsStringExt,
    path::{Path, PathBuf},
    time::Duration,
};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn required(name: &str) -> PathBuf {
    let path = PathBuf::from(
        std::env::var_os(name).unwrap_or_else(|| panic!("mandatory cloud input: {name}")),
    );
    assert!(path.is_absolute() && path.exists());
    path
}
fn regular(path: &Path) {
    assert!(fs::symlink_metadata(path).unwrap().is_file());
    for parent in path.ancestors().skip(1) {
        assert!(fs::symlink_metadata(parent).unwrap().is_dir());
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
const CLI: [&str; 10] = [
    "missing-required",
    "help",
    "help-abbrev",
    "attached-help",
    "attached-value-help",
    "unknown",
    "missing-source-value",
    "missing-output",
    "qemu-without-sysroot",
    "sysroot-without-qemu",
];
fn original() -> (PathBuf, Value) {
    let capture = required("PANDAD_PROTOCOL_ORIGINAL_CAPTURE");
    let path = capture.join("capture-manifest.json");
    assert_eq!(
        hash(&path),
        std::env::var("PANDAD_PROTOCOL_CAPTURE_MANIFEST_SHA256").unwrap()
    );
    let manifest = value(&path);
    assert_eq!(
        manifest["source_commit"],
        std::env::var("PANDAD_PROTOCOL_CAPTURE_SOURCE_COMMIT").unwrap()
    );
    assert_eq!(manifest["borrowed_source_artifact"], 12358686227_u64);
    assert_eq!(
        manifest["borrowed_zip_sha256"],
        "1dffb8ea17d36f68e02b3c59736effbf9667d6225fb13e35eec833f140f321cc"
    );
    assert_eq!(
        manifest["source_binary_sha256"],
        "bc4b133fad240a96212144be82f9946ee417390ad1cd4082b0c009ee680cd14d"
    );
    assert_eq!(
        manifest["original_source_sha256"]["rust/tools/check_pandad_protocol.py"],
        "927164a1021139642d4e17f2f35d3b531dbafc989161fe871ccc614a7c7e51bf"
    );
    assert_eq!(
        manifest["original_source_sha256"]["rust/tools/pandad_protocol_cases.py"],
        "3eac4a39401de9311c5a5ea5211afebac1deede3a055f9d15fdb45d7576ee5bf"
    );
    let corpus = root().join("rust/tools/pandad_protocol_cases.py");
    match fs::symlink_metadata(&corpus) {
        Ok(_) => assert_eq!(
            hash(&corpus),
            manifest["original_source_sha256"]["rust/tools/pandad_protocol_cases.py"]
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            for name in ["check_pandad_protocol.py", "check_pandad_can_io.py"] {
                assert_eq!(
                    fs::symlink_metadata(root().join("rust/tools").join(name))
                        .unwrap_err()
                        .kind(),
                    std::io::ErrorKind::NotFound,
                    "shared corpus still has an original caller"
                );
            }
        }
        Err(error) => panic!("cannot inspect original corpus: {error}"),
    }
    let historical_builder_path = required("PANDAD_PROTOCOL_CAPTURE_BUILDER_MANIFEST");
    assert_eq!(
        hash(&historical_builder_path),
        manifest["source_builder_manifest_sha256"]
    );
    let historical_builder = value(&historical_builder_path);
    assert_eq!(
        historical_builder["binary_sha256"],
        manifest["source_binary_sha256"]
    );
    assert_eq!(
        historical_builder["sources"],
        manifest["source_builder_sources"]
    );
    let sources = manifest["source_builder_sources"].as_object().unwrap();
    assert_eq!(sources.len(), 14);
    for (name, pin) in sources {
        assert_eq!(hash(&root().join(name)), pin.as_str().unwrap());
    }
    let seals = manifest["files_b64"].as_object().unwrap();
    let mut sealed = BTreeSet::new();
    for (name, pin) in seals {
        let relative = PathBuf::from(OsString::from_vec(STANDARD.decode(name).unwrap()));
        assert!(relative
            .components()
            .all(|p| matches!(p, std::path::Component::Normal(_))));
        assert!(sealed.insert(relative.clone()));
        let path = capture.join(relative);
        assert_eq!(hash(&path), pin["sha256"].as_str().unwrap());
        assert_eq!(
            fs::metadata(path).unwrap().len(),
            pin["size"].as_u64().unwrap()
        );
    }
    for name in [
        "seeds.input.jsonl",
        "seeds.stdout",
        "source.input.jsonl",
        "source.stdout",
        "native.input.jsonl",
        "native.stdout",
        "report.json",
    ] {
        assert!(sealed.contains(&PathBuf::from("positive").join(name)));
    }
    for name in CLI {
        assert!(sealed.contains(&PathBuf::from(format!("admissions/{name}.json"))));
    }
    for length in [
        9, 10, 11, 13, 15, 17, 19, 21, 23, 25, 31, 33, 47, 49, 63, 65, 255,
    ] {
        for side in ["source", "native"] {
            assert!(sealed.contains(&PathBuf::from(format!(
                "positive/invalid-{length}-{side}.stderr"
            ))));
            for suffix in ["input.jsonl", "stdout", "stderr"] {
                assert!(sealed.contains(&PathBuf::from(format!(
                    "positive/ignored-{length}-{side}.{suffix}"
                ))));
            }
        }
    }
    let mut actual = BTreeSet::new();
    roster(&capture, &capture, &mut actual);
    assert!(actual.remove(Path::new("capture-manifest.json")));
    assert_eq!(actual, sealed);
    (capture, manifest)
}
// Historical builder identity is verified by original(). This independently
// binds the current executed ELF to its current build receipt and all14 inputs.
fn current_source_binding(historical: &Value, source: &Path) -> Value {
    let path = required("PANDAD_PROTOCOL_SOURCE_MANIFEST");
    let manifest_sha256 = hash(&path);
    let build = value(&path);
    let actual_sha256 = hash(source);
    assert_eq!(build["binary_sha256"], actual_sha256);
    assert_eq!(build["sources"], historical["source_builder_sources"]);
    let sources = build["sources"].as_object().unwrap();
    assert_eq!(sources.len(), 14);
    for (name, expected) in sources {
        assert_eq!(
            hash(&root().join(name)),
            expected.as_str().unwrap(),
            "{name}"
        );
    }
    let kind = std::env::var("PANDAD_PROTOCOL_SOURCE_BUILD_KIND")
        .expect("mandatory explicit fresh/borrowed source build kind");
    match kind.as_str() {
        "fresh" => {
            // A fresh build must name this runner's actual executed binary;
            // matching source hashes alone cannot substitute an arbitrary ELF.
            assert_eq!(Path::new(build["binary"].as_str().unwrap()), source);
        }
        "borrowed" => {
            assert_eq!(
                manifest_sha256,
                historical["source_builder_manifest_sha256"]
            );
            assert_eq!(actual_sha256, historical["source_binary_sha256"]);
            let captured = value(&required("PANDAD_PROTOCOL_CAPTURE_BUILDER_MANIFEST"));
            assert_eq!(build, captured);
        }
        _ => panic!("source build kind must be fresh or borrowed"),
    }
    assert_eq!(
        hash(&path),
        manifest_sha256,
        "current build receipt changed"
    );
    assert_eq!(hash(source), actual_sha256, "current source ELF changed");
    json!({"kind":kind,"manifest_path":path,"manifest_sha256":manifest_sha256,
        "executed_binary":source,"executed_binary_sha256":actual_sha256,
        "historical_binary_sha256":historical["source_binary_sha256"],
        "historical_builder_manifest_sha256":historical["source_builder_manifest_sha256"],
        "current_build":build})
}
fn normalized(path: &Path, requests: &[Value]) -> Vec<Value> {
    let mut values = rows(path);
    assert_eq!(values.len(), requests.len());
    for (request, value) in requests.iter().zip(&mut values) {
        if request["op"] == "alerts" {
            for state in value["results"].as_array_mut().unwrap() {
                for field in ["since", "pending_since"] {
                    if let Some(s) = state[field].as_str() {
                        state[field] = json!(s.parse::<u64>().unwrap());
                    }
                }
                for time in state["times"].as_array_mut().unwrap() {
                    if let Some(s) = time.as_str() {
                        *time = json!(s.parse::<u64>().unwrap());
                    }
                }
            }
        }
    }
    values
}
fn command(source_root: &Path, args: Vec<OsString>) -> CommandSpec {
    let mut argv = vec![env!("CARGO_BIN_EXE_check-pandad-protocol").into()];
    argv.extend(args);
    let mut spec = CommandSpec::new_os(argv, Duration::from_secs(240));
    spec.os_env = Some(
        [
            (
                "OPENPILOT_RUST_ROOT".into(),
                source_root.as_os_str().to_owned(),
            ),
            ("PATH".into(), "/nonexistent".into()),
        ]
        .into_iter()
        .collect(),
    );
    spec.stdout_limit = 1024 * 1024;
    spec.stderr_limit = 1024 * 1024;
    spec
}
fn checkout_mode() -> String {
    let mode = std::env::var("PANDAD_PROTOCOL_RETIREMENT_MODE")
        .expect("explicit predelete/postdelete mode required");
    let checker = root().join("rust/tools/check_pandad_protocol.py");
    match mode.as_str() {
        "predelete" => assert_eq!(
            hash(&checker),
            "927164a1021139642d4e17f2f35d3b531dbafc989161fe871ccc614a7c7e51bf"
        ),
        "postdelete" => {
            for name in [
                "check_pandad_protocol.py",
                "check_pandad_can_io.py",
                "pandad_protocol_cases.py",
            ] {
                assert_eq!(
                    fs::symlink_metadata(root().join("rust/tools").join(name))
                        .unwrap_err()
                        .kind(),
                    std::io::ErrorKind::NotFound
                );
            }
        }
        _ => panic!("unknown protocol retirement mode"),
    };
    mode
}
#[test]
fn full_original_requests_source_responses_invalid_controls_and_reports_match_checkout_and_absent()
{
    let mode = checkout_mode();
    let (capture, manifest) = original();
    let evidence = tempfile::Builder::new()
        .prefix("protocol-pair-")
        .tempdir_in(required("PANDAD_PROTOCOL_TEST_OUTPUT"))
        .unwrap()
        .keep();
    let source = required("PANDAD_PROTOCOL_SOURCE_BINARY");
    let native = required("PANDAD_PROTOCOL_NATIVE_BINARY");
    let checker = PathBuf::from(env!("CARGO_BIN_EXE_check-pandad-protocol"));
    let pins = [hash(&source), hash(&native), hash(&checker)];
    let source_binding = current_source_binding(&manifest, &source);
    fs::write(
        evidence.join("source-build-current.json"),
        serde_json::to_vec_pretty(&source_binding).unwrap(),
    )
    .unwrap();
    let absent = evidence.join("absent-root");
    for name in manifest["source_builder_sources"]
        .as_object()
        .unwrap()
        .keys()
        .chain(
            ["rust/tools/pandad_protocol_cases.py".to_owned()]
                .iter()
                .filter(|name| root().join(name).is_file()),
        )
    {
        let target = absent.join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(root().join(name), target).unwrap();
    }
    let requests = rows(&capture.join("positive/source.input.jsonl"));
    assert_eq!(requests.len(), 3174);
    let expected = normalized(&capture.join("positive/source.stdout"), &requests);
    for (lane, source_root) in [("checkout", root()), ("absent", absent)] {
        let output = evidence.join(lane);
        let child = NativeRunner
            .run(&command(
                &source_root,
                vec![
                    "--source".into(),
                    source.clone().into_os_string(),
                    "--binary".into(),
                    native.clone().into_os_string(),
                    "--output".into(),
                    output.clone().into_os_string(),
                ],
            ))
            .unwrap();
        fs::write(evidence.join(format!("{lane}.stdout")), &child.stdout).unwrap();
        fs::write(evidence.join(format!("{lane}.stderr")), &child.stderr).unwrap();
        assert_eq!(child.code, 0, "{}", String::from_utf8_lossy(&child.stderr));
        assert_eq!(
            rows(&output.join("seeds.input.jsonl")),
            rows(&capture.join("positive/seeds.input.jsonl"))
        );
        assert_eq!(
            rows(&output.join("seeds.stdout")),
            rows(&capture.join("positive/seeds.stdout"))
        );
        for label in ["source", "native"] {
            assert_eq!(rows(&output.join(format!("{label}.input.jsonl"))), requests);
            assert_eq!(
                normalized(&output.join(format!("{label}.stdout")), &requests),
                expected
            );
            assert!(fs::read(output.join(format!("{label}.stderr")))
                .unwrap()
                .is_empty());
        }
        for length in [
            9, 10, 11, 13, 15, 17, 19, 21, 23, 25, 31, 33, 47, 49, 63, 65, 255,
        ] {
            for side in ["source", "native"] {
                let label = format!("ignored-{length}-{side}");
                assert_eq!(
                    rows(&output.join(format!("{label}.input.jsonl"))),
                    rows(&capture.join(format!("positive/{label}.input.jsonl")))
                );
                assert_eq!(
                    rows(&output.join(format!("{label}.stdout"))),
                    vec![json!({"chunks":[]})]
                );
                assert!(
                    !fs::read(output.join(format!("invalid-{length}-{side}.stderr")))
                        .unwrap()
                        .is_empty()
                );
            }
        }
        let mut report = value(&capture.join("positive/report.json"));
        assert_eq!(
            report["source_binary_sha256"],
            manifest["source_binary_sha256"]
        );
        assert_eq!(
            report["native_binary_sha256"],
            manifest["native_binary_sha256"]
        );
        report["source_binary_sha256"] = json!(pins[0]);
        report["native_binary_sha256"] = json!(pins[1]);
        report["native_command"] = json!([native]);
        assert_eq!(value(&output.join("report.json")), report);
        assert_eq!(
            serde_json::from_slice::<Value>(&child.stdout).unwrap(),
            report
        );
        assert_eq!([hash(&source), hash(&native), hash(&checker)], pins);
        assert_eq!(current_source_binding(&manifest, &source), source_binding);
    }
    assert_eq!(original(), (capture, manifest));
    assert_eq!(checkout_mode(), mode);
}
#[test]
fn captured_original_cli_admissions_channels_and_absence_of_effects_match() {
    let mode = checkout_mode();
    let (capture, manifest) = original();
    let private = tempfile::tempdir_in(required("PANDAD_PROTOCOL_TEST_OUTPUT")).unwrap();
    for name in CLI {
        let expected = value(&capture.join(format!("admissions/{name}.json")));
        let argv = expected["argv_b64"]
            .as_array()
            .unwrap()
            .iter()
            .skip(3)
            .map(|a| OsString::from_vec(STANDARD.decode(a.as_str().unwrap()).unwrap()))
            .collect();
        let mut spec = command(&root(), argv);
        spec.cwd = Some(private.path().into());
        let child = NativeRunner.run(&spec).unwrap();
        let receipt_path =
            required("PANDAD_PROTOCOL_TEST_OUTPUT").join(format!("native-cli-{name}.json"));
        fs::write(receipt_path,serde_json::to_vec_pretty(&json!({"case":name,"argv_b64":spec.effective_argv().iter().map(|arg|STANDARD.encode(std::os::unix::ffi::OsStrExt::as_bytes(arg.as_os_str()))).collect::<Vec<_>>(),"exit":child.code,"stdout_b64":STANDARD.encode(&child.stdout),"stderr_b64":STANDARD.encode(&child.stderr)})).unwrap()).unwrap();
        assert_eq!(i64::from(child.code), expected["exit"].as_i64().unwrap());
        assert_eq!(
            child.stdout.is_empty(),
            STANDARD
                .decode(expected["stdout_b64"].as_str().unwrap())
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            child.stderr.is_empty(),
            STANDARD
                .decode(expected["stderr_b64"].as_str().unwrap())
                .unwrap()
                .is_empty()
        );
        assert!(!String::from_utf8_lossy(&child.stdout).contains("\"result\""));
        let original_stdout = STANDARD
            .decode(expected["stdout_b64"].as_str().unwrap())
            .unwrap();
        let original_stderr = STANDARD
            .decode(expected["stderr_b64"].as_str().unwrap())
            .unwrap();
        for bytes in [
            &child.stdout,
            &child.stderr,
            &original_stdout,
            &original_stderr,
        ] {
            assert!(!bytes.windows(4).any(|part| part == b"PASS"));
            assert!(!String::from_utf8_lossy(bytes).contains("\"result\""));
        }
        if child.code == 0 {
            for flag in ["--source", "--binary", "--output", "--qemu", "--sysroot"] {
                assert!(String::from_utf8_lossy(&child.stdout).contains(flag));
                assert!(String::from_utf8_lossy(&original_stdout).contains(flag));
            }
        } else {
            let native_error = String::from_utf8_lossy(&child.stderr);
            let original_error = String::from_utf8_lossy(&original_stderr);
            match name {
                "missing-required" | "missing-output" => {
                    assert!(
                        native_error.contains("required") && original_error.contains("required")
                    );
                }
                "unknown" => {
                    assert!(
                        native_error.contains("required") && original_error.contains("required")
                    );
                }
                "missing-source-value" => {
                    assert!(native_error.contains("--source") && native_error.contains("value"));
                    assert!(
                        original_error.contains("--source") && original_error.contains("argument")
                    );
                }
                "qemu-without-sysroot" | "sysroot-without-qemu" => {
                    for error in [&native_error, &original_error] {
                        assert!(
                            error.contains("--qemu")
                                && error.contains("--sysroot")
                                && error.contains("together")
                        );
                    }
                }
                _ => panic!("unexpected original help/error category"),
            }
        }
        assert_eq!(fs::read_dir(private.path()).unwrap().count(), 0);
    }
    assert_eq!(original(), (capture, manifest));
    assert_eq!(checkout_mode(), mode);
}

#[test]
fn original_failure_order_and_foreign_receipt_effects_match_native() {
    let mode = checkout_mode();
    let (capture, manifest) = original();
    let evidence = tempfile::Builder::new()
        .prefix("protocol-failures-")
        .tempdir_in(required("PANDAD_PROTOCOL_TEST_OUTPUT"))
        .unwrap()
        .keep();
    let source = required("PANDAD_PROTOCOL_SOURCE_BINARY");
    let native = required("PANDAD_PROTOCOL_NATIVE_BINARY");
    let source_binding = current_source_binding(&manifest, &source);
    fs::write(
        evidence.join("source-build-current.json"),
        serde_json::to_vec_pretty(&source_binding).unwrap(),
    )
    .unwrap();
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
        let spec = command(
            &root(),
            vec![
                "--source".into(),
                if name == "missing-source" {
                    absent.clone().into_os_string()
                } else {
                    source.clone().into_os_string()
                },
                "--binary".into(),
                if name == "missing-native" {
                    absent.clone().into_os_string()
                } else {
                    native.clone().into_os_string()
                },
                "--output".into(),
                output.clone().into_os_string(),
            ],
        );
        let child = NativeRunner.run(&spec).unwrap();
        fs::write(evidence.join(format!("{name}.stdout")), &child.stdout).unwrap();
        fs::write(evidence.join(format!("{name}.stderr")), &child.stderr).unwrap();
        assert_eq!(i64::from(child.code), receipt["exit"].as_i64().unwrap());
        assert_eq!(child.code, 1);
        assert!(child.stdout.is_empty());
        assert!(!child.stderr.is_empty());
        let seals = receipt["files_b64"].as_object().unwrap();
        let expected = seals
            .keys()
            .map(|name| PathBuf::from(OsString::from_vec(STANDARD.decode(name).unwrap())))
            .collect::<BTreeSet<_>>();
        let mut actual = BTreeSet::new();
        roster(&output, &output, &mut actual);
        assert_eq!(actual, expected, "{name}: failure order and output roster");
        for relative in expected {
            assert!(relative
                .components()
                .all(|part| matches!(part, std::path::Component::Normal(_))));
            let original_file = capture.join(directory).join(&relative);
            let seal = seals
                .get(&STANDARD.encode(std::os::unix::ffi::OsStrExt::as_bytes(relative.as_os_str())))
                .unwrap();
            assert_eq!(hash(&original_file), seal["sha256"].as_str().unwrap());
            if relative
                .extension()
                .is_some_and(|extension| extension == "jsonl")
            {
                assert_eq!(rows(&output.join(relative)), rows(&original_file));
            } else {
                assert_eq!(
                    fs::read(output.join(relative)).unwrap(),
                    fs::read(original_file).unwrap()
                );
            }
        }
        assert_eq!(current_source_binding(&manifest, &source), source_binding);
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
    assert_eq!(checkout_mode(), mode);
}

fn python_report(path: &Path) -> openpilot_log_format::JsonValue {
    openpilot_log_format::JsonValue::parse(&fs::read_to_string(path).unwrap()).unwrap()
}
#[test]
fn captured_actual_raw_native_executable_preserves_full_command_and_report() {
    use openpilot_log_format::JsonView;
    let mode = checkout_mode();
    let (capture, manifest) = original();
    let evidence = tempfile::Builder::new()
        .prefix("protocol-raw-native-")
        .tempdir_in(required("PANDAD_PROTOCOL_TEST_OUTPUT"))
        .unwrap()
        .keep();
    let source = required("PANDAD_PROTOCOL_SOURCE_BINARY");
    let native = required("PANDAD_PROTOCOL_NATIVE_BINARY");
    let native_pin = hash(&native);
    let source_pin = hash(&source);
    let source_binding = current_source_binding(&manifest, &source);
    fs::write(
        evidence.join("source-build-current.json"),
        serde_json::to_vec_pretty(&source_binding).unwrap(),
    )
    .unwrap();
    let original_path = OsString::from_vec(
        STANDARD
            .decode(manifest["raw_native"]["path_b64"].as_str().unwrap())
            .unwrap(),
    );
    assert_eq!(manifest["raw_native"]["archived_as"], "raw-native-elf");
    assert_eq!(
        hash(&capture.join("raw-native-elf")),
        manifest["raw_native"]["sha256"].as_str().unwrap()
    );
    let original_report = python_report(&capture.join("raw-native-output/report.json"));
    let JsonView::Array(original_command) = original_report.get("native_command").unwrap().view()
    else {
        panic!("original command array missing")
    };
    assert_eq!(original_command.len(), 1);
    assert_eq!(
        openpilot_dev_tools::python_fs::from_json(&original_command[0]).unwrap(),
        original_path
    );
    let original_admission = value(&capture.join("admissions/raw-native.json"));
    assert_eq!(original_admission["exit"], 0);
    let argv = original_admission["argv_b64"].as_array().unwrap();
    assert_eq!(
        OsString::from_vec(STANDARD.decode(argv[6].as_str().unwrap()).unwrap()),
        original_path,
        "actual original --binary argv"
    );
    let raw_native = evidence.join(OsString::from_vec(b"native-\xff".to_vec()));
    fs::hard_link(&native, &raw_native).unwrap();
    let output = evidence.join("output");
    let spec = command(
        &root(),
        vec![
            "--source".into(),
            source.clone().into_os_string(),
            "--binary".into(),
            raw_native.clone().into_os_string(),
            "--output".into(),
            output.clone().into_os_string(),
        ],
    );
    let child = NativeRunner.run(&spec).unwrap();
    fs::write(evidence.join("native.stdout"), &child.stdout).unwrap();
    fs::write(evidence.join("native.stderr"), &child.stderr).unwrap();
    fs::write(evidence.join("native-invocation.json"),serde_json::to_vec_pretty(&json!({"argv_b64":spec.effective_argv().iter().map(|arg|STANDARD.encode(std::os::unix::ffi::OsStrExt::as_bytes(arg.as_os_str()))).collect::<Vec<_>>(),"exit":child.code,"stdout_b64":STANDARD.encode(&child.stdout),"stderr_b64":STANDARD.encode(&child.stderr)})).unwrap()).unwrap();
    assert_eq!(child.code, 0, "{}", String::from_utf8_lossy(&child.stderr));
    assert!(child.stderr.is_empty());
    let actual = python_report(&output.join("report.json"));
    let printed =
        openpilot_log_format::JsonValue::parse(std::str::from_utf8(&child.stdout).unwrap())
            .unwrap();
    assert_eq!(actual.to_json().unwrap(), printed.to_json().unwrap());
    let JsonView::Object(fields) = original_report.view() else {
        panic!("original report object missing")
    };
    let JsonView::Object(actual_fields) = actual.view() else {
        panic!("native report object missing")
    };
    assert_eq!(fields.len(), actual_fields.len());
    for (key, want) in fields {
        let name = key
            .iter()
            .map(|&point| char::from_u32(point).unwrap())
            .collect::<String>();
        let got = actual.get(&name).unwrap();
        if name == "native_command" {
            let JsonView::Array(command) = got.view() else {
                panic!("native command missing")
            };
            assert_eq!(command.len(), 1);
            assert_eq!(
                openpilot_dev_tools::python_fs::from_json(&command[0]).unwrap(),
                raw_native.as_os_str()
            );
        } else if name == "native_binary_sha256" {
            assert_eq!(got.to_utf8().unwrap(), native_pin);
        } else if name == "source_binary_sha256" {
            assert_eq!(
                want.to_utf8().unwrap(),
                manifest["source_binary_sha256"].as_str().unwrap()
            );
            assert_eq!(got.to_utf8().unwrap(), source_pin);
        } else {
            assert_eq!(
                serde_json::from_str::<Value>(&got.to_json().unwrap()).unwrap(),
                serde_json::from_str::<Value>(&want.to_json().unwrap()).unwrap(),
                "original report field {name}"
            );
        }
    }
    let requests = rows(&capture.join("raw-native-output/source.input.jsonl"));
    assert_eq!(requests.len(), 3174);
    let expected = normalized(&capture.join("raw-native-output/source.stdout"), &requests);
    for label in ["source", "native"] {
        assert_eq!(rows(&output.join(format!("{label}.input.jsonl"))), requests);
        assert_eq!(
            normalized(&output.join(format!("{label}.stdout")), &requests),
            expected
        );
    }
    assert_eq!(hash(&raw_native), native_pin);
    assert_eq!(hash(&native), native_pin);
    assert_eq!(hash(&source), source_pin);
    assert_eq!(current_source_binding(&manifest, &source), source_binding);
    fs::rename(raw_native, evidence.join("raw-native-elf")).unwrap();
    assert_eq!(original(), (capture, manifest));
    assert_eq!(checkout_mode(), mode);
}
