//! Independently captured current encoder CI selection, ordering and failure phases.
#![cfg(feature = "encoder-runtime-native")]
use openpilot_dev_tools::{
    encoder_ci_check::{dependency_outputs, execute, Options},
    qa_process::{Captured, CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};
fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}
fn dependencies(root: &Path) {
    for relative in [
        "jpeg-current/jpeg/libjpeg.a",
        "jpeg-current/jpeg/jconfig.h",
        "zmq-current/source/include/zmq.h",
        "zmq-current/lib/libzmq.a",
    ] {
        write(&root.join(relative), b"owned fixture");
    }
}
fn expand(value: &str, root: &Path) -> String {
    value.replace("$ROOT", root.to_str().unwrap())
}
fn normalize(value: &Value, root: &Path) -> Value {
    serde_json::from_str(
        &serde_json::to_string(value)
            .unwrap()
            .replace(root.to_str().unwrap(), "$ROOT"),
    )
    .unwrap()
}
#[test]
fn independent_original_captures_keep_source_identity() {
    use sha2::{Digest, Sha256};
    for (text, digest) in [
        (
            include_str!("fixtures/encoder-ci-check/dependency-original.json"),
            "b42b48448fd598be359dc2a59bb4e628b328f0888d1f54609abba13f1b2d4921",
        ),
        (
            include_str!("fixtures/encoder-ci-check/main-original.json"),
            "009f57775d80d6bb07de5de2a341305de0be156512291f2a8e48ac89c3c21bc0",
        ),
        (
            include_str!("fixtures/encoder-ci-check/process-original.json"),
            "fa93780a01cf0cffee15d1b6d37c0d10cfb25b679603ff08dfd336cbecaf8f38",
        ),
    ] {
        assert_eq!(format!("{:x}", Sha256::digest(text.as_bytes())), digest);
    }
}
#[test]
fn original_current_cargo_outputs_and_rejections() {
    let corpus: Value = serde_json::from_str(include_str!(
        "fixtures/encoder-ci-check/dependency-original.json"
    ))
    .unwrap();
    for case in corpus["cases"].as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        dependencies(root);
        let input = root.join("build.jsonl");
        write(&input, expand(case["messages"].as_str().unwrap(), root));
        if let Some(path) = case["missing"].as_str() {
            fs::remove_file(expand(path, root)).unwrap();
        }
        let result = dependency_outputs(&input);
        if case["error"].is_null() {
            assert_eq!(
                normalize(&serde_json::to_value(result.unwrap()).unwrap(), root),
                case["result"],
                "{}",
                case["name"]
            );
        } else {
            let message = result.unwrap_err().to_string();
            if matches!(case["error"]["type"].as_str().unwrap(), "ValueError") {
                assert!(
                    message.contains(
                        case["error"]["message"]
                            .as_str()
                            .unwrap()
                            .split(", got")
                            .next()
                            .unwrap()
                    ),
                    "{}: {message}",
                    case["name"]
                );
            }
        }
    }
}
fn options(root: &Path) -> Options {
    Options {
        root: root.into(),
        binaries: root.join("bin"),
        build_messages: root.join("build.jsonl"),
        output: root.join("output"),
        native: root.join("native"),
        capnp: root.join("capnp"),
        json11_prefix: None,
        json11_stager: Some(root.join("stage-json11")),
    }
}
fn setup(root: &Path) {
    dependencies(root);
    let lines = [
        json!({"reason":"build-script-executed","package_id":"path+x#openpilot-jpeg@0.1","out_dir":root.join("jpeg-current")}),
        json!({"reason":"build-script-executed","package_id":"path+x#zmq-sys@0.1","out_dir":root.join("zmq-current")}),
        json!({"reason":"build-finished","success":true}),
    ];
    write(
        &root.join("build.jsonl"),
        lines.iter().map(|v| format!("{v}\n")).collect::<String>(),
    );
    for name in [
        "openpilot-encoderd",
        "examples/encoder_codec_trace",
        "examples/encoder_v4l_trace",
        "build-encoder-source",
        "check-encoder-codecs",
        "check-encoder-v4l",
        "check-encoder-runtime",
        "check-encoder-ci",
    ] {
        write(&root.join("bin").join(name), format!("executable:{name}"));
    }
    for path in [
        "rust/Cargo.lock",
        "uv.lock",
        "rust/crates/dev-tools/Cargo.toml",
        "rust/crates/messaging/src/services_generated.rs",
        "rust/crates/dev-tools/src/encoder_source_build.rs",
        "rust/crates/dev-tools/src/encoder_codecs_check.rs",
        "rust/crates/dev-tools/src/encoder_v4l_check.rs",
        "rust/crates/dev-tools/src/encoder_runtime_native.rs",
        "rust/crates/dev-tools/src/encoder_runtime_wire.rs",
        "rust/crates/dev-tools/src/encoder_runtime_check.rs",
        "rust/crates/dev-tools/src/encoder_runtime_exercise.rs",
        "rust/crates/dev-tools/src/encoder_runtime_process.rs",
        "rust/crates/dev-tools/src/encoder_ci_check.rs",
        "rust/crates/dev-tools/src/qa_process.rs",
        "rust/crates/dev-tools/src/services_header.rs",
        "rust/crates/dev-tools/src/path_resolve.rs",
        "rust/crates/dev-tools/src/python_fs.rs",
        "rust/crates/dev-tools/src/bin/build_encoder_source.rs",
        "rust/crates/dev-tools/src/bin/check_encoder_codecs.rs",
        "rust/crates/dev-tools/src/bin/check_encoder_v4l.rs",
        "rust/crates/dev-tools/src/bin/check_encoder_runtime.rs",
        "rust/crates/dev-tools/src/bin/check_encoder_ci.rs",
        "rust/crates/encoderd/src/lib.rs",
        "rust/crates/msgq/src/lib.rs",
        "rust/crates/jpeg/src/lib.rs",
        "rust/tools/encoder_oracle/fixture.cc",
        "openpilot/cereal/log.capnp",
        "openpilot/cereal/deprecated.capnp",
        "openpilot/cereal/custom.capnp",
        "openpilot/cereal/include/c++.capnp",
        "opendbc_repo/opendbc/car/car.capnp",
        "openpilot/system/loggerd/encoderd.cc",
        "msgq_repo/msgq/visionipc/visionipc.h",
    ] {
        write(&root.join(path), format!("fixture:{path}"));
    }
}
struct Peer {
    root: PathBuf,
    scenario: String,
    calls: std::rc::Rc<std::cell::RefCell<Vec<Value>>>,
}
impl Runner for Peer {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        assert!(spec.inherit_stdin, "original subprocess.run inherits stdin");
        let argv = spec
            .effective_argv()
            .iter()
            .map(|v| v.to_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        let output = if argv[0] == "git" || argv[0] == "pkg-config" {
            self.calls.borrow_mut().push(json!(["output", argv]));
            if argv[0] == "git" {
                "0123456789abcdef\n".to_owned()
            } else {
                "60.1\n60.2\n58.3\n".to_owned()
            }
        } else {
            let name = match Path::new(&argv[0]).file_name().unwrap().to_str().unwrap() {
                "stage-json11" => "stage-json11",
                "build-encoder-source" => "source-build",
                "check-encoder-codecs" => "codecs",
                "check-encoder-v4l" => "v4l",
                "check-encoder-runtime" => "runtime",
                n => panic!("unexpected command {n}"),
            };
            self.calls.borrow_mut().push(json!([
                "run",
                name,
                argv,
                self.root.join("output/commands")
            ]));
            assert_eq!(spec.cwd.as_deref(), Some(self.root.as_path()));
            if self.scenario
                == match name {
                    "stage-json11" => "stage-failed".to_owned(),
                    "source-build" => "source-failed".to_owned(),
                    _ => format!("{name}-failed"),
                }
            {
                return Ok(Captured {
                    code: 9,
                    stdout: b"original out\r\n".to_vec(),
                    stderr: b"diagnostic\r\n".to_vec(),
                });
            }
            if name == "stage-json11" {
                if self.scenario == "stage-invalid" {
                    "not-json".into()
                } else {
                    json!({"prefix":self.root.join("json11-prefix"),"dependency":{"json11_sha256":"b".repeat(64)}}).to_string()
                }
            } else {
                if ["codecs", "v4l", "runtime"].contains(&name) {
                    let mut count = match name {
                        "codecs" => 5,
                        "v4l" => 32,
                        _ => 10,
                    };
                    if self.scenario == format!("{name}-short") {
                        count -= 1;
                    }
                    write(&self.root.join("output").join(name).join("receipt.json"),json!({"status":if name=="runtime" && self.scenario=="runtime-status" {"FAIL"} else {"PASS"},"results":vec![json!({});count]}).to_string());
                }
                String::new()
            }
        };
        Ok(Captured {
            code: 0,
            stdout: output.into_bytes(),
            stderr: vec![],
        })
    }
}
#[test]
fn original_main_stages_receipts_and_failure_phases() {
    let corpus: Value =
        serde_json::from_str(include_str!("fixtures/encoder-ci-check/main-original.json")).unwrap();
    for case in corpus["cases"].as_array().unwrap() {
        let scenario = case["name"].as_str().unwrap();
        // Original Python import-location admission is replaced by mandatory native
        // checker executable and complete Rust/cereal source identity below.
        if scenario == "outside-binding" {
            continue;
        }
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        setup(root);
        let mut opts = options(root);
        if scenario == "prestaged" {
            opts.json11_prefix = Some(root.join("already-json11"));
        }
        if scenario == "jpeg-output-missing" {
            fs::remove_file(root.join("jpeg-current/jpeg/libjpeg.a")).unwrap();
        }
        let calls = std::rc::Rc::new(std::cell::RefCell::new(vec![]));
        let mut peer = Peer {
            root: root.into(),
            scenario: scenario.into(),
            calls: calls.clone(),
        };
        let observed = calls.clone();
        let mut space = |path: &Path, growth: u64| -> Result<()> {
            observed.borrow_mut().push(json!(["space", path, growth]));
            if (scenario == "space-initial" && observed.borrow().len() == 1)
                || (scenario == "space-stage" && growth == 256 * 1024 * 1024)
            {
                return Err("fixture capacity".into());
            }
            Ok(())
        };
        let mut out = vec![];
        let mut err = vec![];
        let result = execute(&opts, &mut peer, &mut space, &mut out, &mut err);
        assert_eq!(
            result.is_err(),
            !case["error"].is_null(),
            "{scenario}: {result:?}"
        );
        let mut expected = case["calls"].clone();
        for call in expected.as_array_mut().unwrap() {
            if call[0] == "run" && call[1] == "runtime" {
                let old = call[2].as_array().unwrap();
                let mut argv = vec![json!("$ROOT/bin/check-encoder-runtime")];
                argv.extend(old[2..].iter().cloned());
                call[2] = json!(argv);
            }
        }
        assert_eq!(
            normalize(&json!(*calls.borrow()), root),
            expected,
            "{scenario} call order"
        );
        assert_eq!(
            opts.output.join("receipt.json").exists(),
            case["error"].is_null(),
            "{scenario}"
        );
        for (path, expected) in case["artifacts"].as_object().unwrap() {
            assert_eq!(
                normalize(
                    &serde_json::from_slice::<Value>(&fs::read(opts.output.join(path)).unwrap())
                        .unwrap(),
                    root
                ),
                serde_json::from_str::<Value>(expected.as_str().unwrap()).unwrap(),
                "{scenario} {path}"
            );
        }
        if opts.output.join("inputs.json").exists() {
            let identity: Value =
                serde_json::from_slice(&fs::read(opts.output.join("inputs.json")).unwrap())
                    .unwrap();
            assert_eq!(identity["source_commit"], "0123456789abcdef");
            assert_eq!(identity["native_codec_versions"], "60.1\n60.2\n58.3\n");
            assert!(identity.get("python_packages").is_none());
            assert!(identity.get("imports").is_none());
            for required in [
                "rust/crates/dev-tools/src/encoder_runtime_native.rs",
                "rust/crates/dev-tools/src/encoder_runtime_wire.rs",
                "rust/crates/dev-tools/src/encoder_ci_check.rs",
                "rust/crates/dev-tools/src/python_fs.rs",
                "openpilot/cereal/log.capnp",
                "openpilot/system/loggerd/encoderd.cc",
                "msgq_repo/msgq/visionipc/visionipc.h",
            ] {
                assert!(identity["sources"][required].is_string(), "{required}");
            }
            assert_eq!(identity["executables"].as_object().unwrap().len(), 8);
            use sha2::{Digest, Sha256};
            assert_eq!(
                identity["build_messages_sha256"],
                format!(
                    "{:x}",
                    Sha256::digest(fs::read(root.join("build.jsonl")).unwrap())
                )
            );
            assert_eq!(
                normalize(&identity["dependencies"], root),
                json!({"jpeg":"$ROOT/jpeg-current/jpeg","zmq":"$ROOT/zmq-current"})
            );
            for (relative, expected) in identity["sources"].as_object().unwrap() {
                assert_eq!(
                    expected.as_str().unwrap(),
                    format!(
                        "{:x}",
                        Sha256::digest(fs::read(root.join(relative)).unwrap())
                    ),
                    "{relative}"
                );
            }
            for (path, expected) in identity["executables"].as_object().unwrap() {
                assert_eq!(
                    expected.as_str().unwrap(),
                    format!("{:x}", Sha256::digest(fs::read(path).unwrap())),
                    "{path}"
                );
            }
        }
        if scenario.ends_with("-failed") {
            assert_eq!(
                String::from_utf8(err).unwrap(),
                "original out\ndiagnostic\n\n"
            );
        }
    }
}
#[test]
fn missing_native_runtime_fingerprint_precedes_builds() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    setup(root);
    fs::remove_file(root.join("bin/check-encoder-runtime")).unwrap();
    let calls = std::rc::Rc::new(std::cell::RefCell::new(vec![]));
    let mut peer = Peer {
        root: root.into(),
        scenario: "success".into(),
        calls: calls.clone(),
    };
    let result = execute(
        &options(root),
        &mut peer,
        &mut |_, _| Ok(()),
        &mut vec![],
        &mut vec![],
    );
    assert!(result.is_err());
    assert!(!root.join("output/receipt.json").exists());
    assert!(!calls.borrow().iter().any(|v| v[0] == "run"));
}
#[test]
fn owned_process_stager_builder_artifacts_and_failure_short_circuit() {
    use std::os::unix::fs::PermissionsExt;
    let corpus: Value = serde_json::from_str(include_str!(
        "fixtures/encoder-ci-check/process-original.json"
    ))
    .unwrap();
    for case in corpus["cases"].as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        setup(root);
        let mut opts = options(root);
        opts.output = root.join("owned output");
        let name = case["name"].as_str().unwrap();
        let prefix = root.join("native-prefix");
        let mut receipt = json!({"prefix":prefix,"dependency":{"json11_sha256":"b".repeat(64)}});
        if name == "stage-no-prefix" {
            receipt.as_object_mut().unwrap().remove("prefix");
        }
        if name == "stage-no-dependency" {
            receipt.as_object_mut().unwrap().remove("dependency");
        }
        let text = case["artifacts"]["commands/stage-json11.stdout"]
            .as_str()
            .map(|v| expand(v, root))
            .unwrap_or_else(|| receipt.to_string());
        let binary = root.join("stage-json11");
        write(
            &binary,
            format!(
                "#!/bin/sh\nprintf %s '{}'\nprintf diagnostic >&2\nexit {}\n",
                text.replace('\'', "'\\''"),
                if name == "stage-exit9" { 9 } else { 0 }
            ),
        );
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
        let builder = root.join("bin/build-encoder-source");
        write(
            &builder,
            format!(
                "#!/bin/sh\nprintf \"%s\\n\" \"$@\"\nprintf diagnostic >&2\nexit {}\n",
                if name == "builder-exit9" { 9 } else { 0 }
            ),
        );
        fs::set_permissions(builder, fs::Permissions::from_mode(0o755)).unwrap();
        if name == "prestaged" {
            opts.json11_prefix = Some(prefix.clone());
        }
        if name == "no-stager" {
            opts.json11_stager = None;
        }
        let mut calls = vec![];
        let mut space = |path: &Path, growth: u64| -> Result<()> {
            calls.push(json!(["space", path, growth]));
            Ok(())
        };
        let mut out = vec![];
        let mut errors = vec![];
        let result = if name.starts_with("builder") {
            let deps = [
                ("zmq".into(), root.join("current-zmq")),
                ("jpeg".into(), root.join("current-jpeg")),
            ]
            .into();
            openpilot_dev_tools::encoder_ci_check::build_source(
                &opts,
                &deps,
                &root.join("json11"),
                &mut NativeRunner,
                &mut space,
                &mut out,
                &mut errors,
            )
            .map(|_| None)
        } else {
            openpilot_dev_tools::encoder_ci_check::json11_dependency(
                &opts,
                &mut NativeRunner,
                &mut space,
                &mut out,
                &mut errors,
            )
            .map(Some)
        };
        assert_eq!(
            result.is_err(),
            !case["error"].is_null(),
            "{name}: {result:?}"
        );
        if let Ok(Some(path)) = result {
            assert_eq!(normalize(&json!(path), root), case["result"]);
        }
        assert_eq!(normalize(&json!(calls), root), case["calls"]);
        assert_eq!(
            normalize(&json!(String::from_utf8(out).unwrap()), root),
            case["stdout"]
        );
        assert_eq!(
            normalize(&json!(String::from_utf8(errors).unwrap()), root),
            // Original subprocess captures ran with the real repository root;
            // the native boundary uses this owned test root in the same slot.
            json!(case["stderr"].as_str().unwrap().replace("$REPO", "$ROOT"))
        );
        for (relative, expected) in case["artifacts"].as_object().unwrap() {
            let actual = fs::read_to_string(opts.output.join(relative)).unwrap();
            let expected = expected
                .as_str()
                .unwrap()
                .replace("$REPO", "$ROOT")
                .replace("native-stager", "stage-json11");
            if relative.ends_with(".json") {
                assert_eq!(
                    normalize(&serde_json::from_str::<Value>(&actual).unwrap(), root),
                    serde_json::from_str::<Value>(&expected).unwrap(),
                    "{name} {relative}"
                );
            } else {
                assert_eq!(
                    actual.replace(root.to_str().unwrap(), "$ROOT"),
                    expected,
                    "{name} {relative}"
                );
            }
        }
    }
}

#[test]
fn raw_json11_prefix_survives_owned_stager_and_dependency_metadata() {
    use std::{
        ffi::OsString,
        os::unix::{ffi::OsStringExt, fs::PermissionsExt},
    };
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let mut opts = options(root);
    opts.output = root.join(OsString::from_vec(b"output-\xff".to_vec()));
    let prefix = opts.output.join("json11-dependency/prefix");
    let receipt = format!(
        "{{\"prefix\":{},\"dependency\":{{\"json11_sha256\":\"{}\"}}}}",
        openpilot_dev_tools::python_fs::json_string(prefix.as_os_str()).unwrap(),
        "b".repeat(64),
    );
    let stager = root.join("stage-json11");
    write(&stager, format!("#!/bin/sh\nprintf %s '{receipt}'\n"));
    fs::set_permissions(&stager, fs::Permissions::from_mode(0o755)).unwrap();
    let mut reservations = Vec::new();
    let mut printed = Vec::new();
    let mut errors = Vec::new();
    let actual = openpilot_dev_tools::encoder_ci_check::json11_dependency(
        &opts,
        &mut NativeRunner,
        &mut |path, growth| {
            reservations.push((path.to_path_buf(), growth));
            Ok(())
        },
        &mut printed,
        &mut errors,
    )
    .expect("raw JSON11 prefix must reach the encoder source builder unchanged");
    assert_eq!(actual, prefix);
    assert_eq!(reservations, [(opts.output.clone(), 256 * 1024 * 1024)]);
    assert_eq!(printed, b"PASS stage-json11\n");
    assert!(errors.is_empty());
    assert_eq!(
        fs::read_to_string(opts.output.join("json11.json")).unwrap(),
        format!("{{\"json11_sha256\": \"{}\"}}\n", "b".repeat(64))
    );
    assert_eq!(
        fs::read_to_string(opts.output.join("commands/stage-json11.stdout")).unwrap(),
        receipt
    );
    assert_eq!(
        fs::read_to_string(opts.output.join("commands/stage-json11.exit.json")).unwrap(),
        "{\"returncode\": 0}\n"
    );
    assert!(!opts.output.join("receipt.json").exists());
}
