use openpilot_dev_tools::{
    logging_oracle_build,
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap()
}
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/native-build-closure")
        .join(name)
}
fn replace(value: &Value, pairs: &[(&str, &str)]) -> Value {
    match value {
        Value::String(s) => {
            Value::String(pairs.iter().fold(s.clone(), |s, (a, b)| s.replace(a, b)))
        }
        Value::Array(a) => Value::Array(a.iter().map(|v| replace(v, pairs)).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, v)| {
                    (
                        replace(&json!(k), pairs).as_str().unwrap().to_owned(),
                        replace(v, pairs),
                    )
                })
                .collect(),
        ),
        _ => value.clone(),
    }
}
// Original Path.glob and native read_dir both preserve the current filesystem's
// order. Frozen captures establish membership and argument positions, not the
// directory ordering of the machine on which the capture was produced.
fn original_globs_on_current_filesystem(value: &Value, output: &Path) -> Value {
    match value {
        Value::Array(values) => {
            let mut values: Vec<_> = values
                .iter()
                .map(|value| original_globs_on_current_filesystem(value, output))
                .collect();
            for (directory, suffix) in [("schema", ".capnp"), ("cereal/gen/cpp", ".capnp.c++")] {
                let prefix = format!("$OUTPUT/{directory}/");
                let slots: Vec<_> = values
                    .iter()
                    .enumerate()
                    .filter_map(|(at, value)| {
                        value
                            .as_str()
                            .filter(|s| s.starts_with(&prefix) && s.ends_with(suffix))
                            .map(|_| at)
                    })
                    .collect();
                if slots.is_empty() {
                    continue;
                }
                let live: Vec<_> = fs::read_dir(output.join(directory))
                    .unwrap()
                    .map(|entry| entry.unwrap().file_name().to_str().unwrap().to_owned())
                    .filter(|name| name.ends_with(suffix))
                    .map(|name| format!("{prefix}{name}"))
                    .collect();
                let mut frozen: Vec<_> = slots
                    .iter()
                    .map(|at| values[*at].as_str().unwrap().to_owned())
                    .collect();
                let mut membership = live.clone();
                frozen.sort();
                membership.sort();
                assert_eq!(
                    frozen, membership,
                    "original glob membership must remain exact"
                );
                for (at, path) in slots.into_iter().zip(live) {
                    values[at] = json!(path);
                }
            }
            Value::Array(values)
        }
        Value::Object(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        original_globs_on_current_filesystem(value, output),
                    )
                })
                .collect(),
        ),
        _ => value.clone(),
    }
}
fn setup(source: &Path) {
    fs::create_dir_all(source.join("rust/tools")).unwrap();
    for name in ["openpilot", "msgq_repo", "opendbc_repo"] {
        symlink(root().join(name), source.join(name)).unwrap();
    }
    for name in [
        "bootlog_paths.cc",
        "logger_identifier_source.cc",
        "loggerd_native_producer.cc",
    ] {
        symlink(
            root().join("rust/tools").join(name),
            source.join("rust/tools").join(name),
        )
        .unwrap();
    }
    fs::copy(fixture("json11-lock.toml"), source.join("uv.lock")).unwrap();
    fs::create_dir_all(source.join("rust/target/debug/build/zmq-sys-fixture/out/source/include"))
        .unwrap();
}
struct Peer {
    commands: Vec<Vec<String>>,
    compiler_env: Vec<Value>,
    failure: Option<usize>,
}
impl Runner for Peer {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        self.commands.push(spec.argv.clone());
        if !spec.env.is_empty() {
            self.compiler_env.push(json!({"command":spec.argv,"LD_LIBRARY_PATH":spec.env["LD_LIBRARY_PATH"],"PATH_prefix":spec.env["PATH"].split(':').next().unwrap()}));
        }
        let code = if self.failure == Some(self.commands.len()) {
            9
        } else {
            0
        };
        if code == 0 {
            if spec.argv[0] == "curl" {
                fs::copy(
                    fixture(spec.argv[2].rsplit('/').next().unwrap()),
                    &spec.argv[4],
                )?;
            } else if spec.argv.get(1).is_some_and(|a| a == "compile") {
                let generated = spec
                    .argv
                    .iter()
                    .find_map(|a| a.strip_prefix("-oc++:"))
                    .unwrap();
                for path in spec.argv.iter().filter(|a| a.ends_with(".capnp")) {
                    let name = Path::new(path);
                    fs::write(
                        Path::new(generated).join(format!(
                            "{}.c++",
                            name.file_name().unwrap().to_str().unwrap()
                        )),
                        format!(
                            "controlled generated {}\n",
                            name.file_stem().unwrap().to_str().unwrap()
                        ),
                    )?;
                }
            } else {
                let at = spec.argv.iter().position(|a| a == "-o").unwrap();
                fs::write(&spec.argv[at + 1], b"controlled compiler product\n")?;
            }
        }
        Ok(Captured {
            code,
            stdout: if spec.merge_output {
                b"controlled stdout\ncontrolled stderr\n".to_vec()
            } else {
                b"controlled stdout\n".to_vec()
            },
            stderr: if spec.merge_output {
                vec![]
            } else {
                b"controlled stderr\n".to_vec()
            },
        })
    }
}
fn hashes(path: &Path, base: &Path, result: &mut serde_json::Map<String, Value>) {
    for entry in fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            hashes(&path, base, result);
        } else {
            result.insert(
                path.strip_prefix(base).unwrap().to_str().unwrap().into(),
                json!(format!("{:x}", Sha256::digest(fs::read(&path).unwrap()))),
            );
        }
    }
}
#[test]
fn eight_original_bootlog_identifier_loggerd_cases_match_compiler_and_source_contracts() {
    let cases: Vec<Value> =
        serde_json::from_slice(&fs::read(fixture("logging-oracles-original.json")).unwrap())
            .unwrap();
    for case in cases {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("root");
        let output = temp.path().join("output");
        setup(&source);
        let args: Vec<String> = serde_json::from_value(replace(
            &case["args"],
            &[
                ("$OUTPUT", output.to_str().unwrap()),
                ("$ROOT", source.to_str().unwrap()),
                ("$DEPS", temp.path().to_str().unwrap()),
            ],
        ))
        .unwrap();
        let mut peer = Peer {
            commands: vec![],
            compiler_env: vec![],
            failure: None,
        };
        logging_oracle_build::run(
            &source,
            case["family"].as_str().unwrap(),
            &args,
            &source.join("rust/target"),
            "x86_64",
            &mut peer,
            u64::MAX,
        )
        .unwrap();
        let normalize = |v: &Value| {
            replace(
                v,
                &[
                    (output.to_str().unwrap(), "$OUTPUT"),
                    (source.to_str().unwrap(), "$ROOT"),
                    (temp.path().to_str().unwrap(), "$DEPS"),
                ],
            )
        };
        let expected_commands: Vec<Value> = case["commands"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|command| command[0] != "python3")
            .cloned()
            .collect();
        assert_eq!(
            normalize(&json!(peer.commands)),
            original_globs_on_current_filesystem(&json!(expected_commands), &output)
        );
        assert_eq!(
            normalize(&json!(peer.compiler_env)),
            original_globs_on_current_filesystem(&case["compiler_env"], &output)
        );
        for (name, expected) in case["files"].as_object().unwrap() {
            let text = fs::read_to_string(output.join(name)).unwrap();
            let value = if name.ends_with(".json") {
                serde_json::from_str(&text).unwrap()
            } else {
                json!(text)
            };
            assert_eq!(
                normalize(&value),
                original_globs_on_current_filesystem(expected, &output),
                "{} variant{} {name}",
                case["family"],
                case["variant"]
            );
        }
        let mut schema = serde_json::Map::new();
        hashes(&output.join("schema"), &output, &mut schema);
        assert_eq!(json!(schema), case["schema_hashes"]);
        if case["family"] == "loggerd" {
            let provenance: Value =
                serde_json::from_slice(&fs::read(output.join("services-provenance.json")).unwrap())
                    .unwrap();
            assert_eq!(
                provenance["header_sha256"],
                format!(
                    "{:x}",
                    Sha256::digest(fs::read(output.join("cereal/services.h")).unwrap())
                )
            );
        }
    }
}
#[test]
fn bootlog_strict_schema_and_failed_compiler_keep_original_partial_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("root");
    let output = temp.path().join("output");
    setup(&source);
    let args = vec![
        output.to_str().unwrap().into(),
        "--json11-prefix".into(),
        temp.path().join("json11").to_str().unwrap().into(),
    ];
    let mut peer = Peer {
        commands: vec![],
        compiler_env: vec![],
        failure: Some(2),
    };
    assert!(logging_oracle_build::run(
        &source,
        "bootlog",
        &args,
        &source.join("rust/target"),
        "x86_64",
        &mut peer,
        u64::MAX
    )
    .is_err());
    assert!(output.join("command.json").exists());
    assert!(output.join("build.log").exists());
    assert!(!output.join("source-hashes.json").exists());
    let count = peer.commands.len();
    assert!(logging_oracle_build::run(
        &source,
        "bootlog",
        &args,
        &source.join("rust/target"),
        "x86_64",
        &mut peer,
        u64::MAX
    )
    .is_err());
    assert_eq!(peer.commands.len(), count);
}
#[test]
fn loggerd_producer_failure_omits_late_success_receipts_and_disk_guard_blocks_commands() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("root");
    let output = temp.path().join("output");
    setup(&source);
    let args = vec!["--output".into(), output.to_str().unwrap().into()];
    let mut peer = Peer {
        commands: vec![],
        compiler_env: vec![],
        failure: Some(4),
    };
    assert!(logging_oracle_build::run(
        &source,
        "loggerd",
        &args,
        &source.join("rust/target"),
        "x86_64",
        &mut peer,
        0
    )
    .is_err());
    assert!(peer.commands.is_empty());
    assert!(logging_oracle_build::run(
        &source,
        "loggerd",
        &args,
        &source.join("rust/target"),
        "x86_64",
        &mut peer,
        u64::MAX
    )
    .is_err());
    assert!(output.join("build-command.json").exists());
    assert!(output.join("producer-build.log").exists());
    assert!(!output.join("producer-build-command.json").exists());
    assert!(!output.join("source-hashes.json").exists());
    assert!(!output.join("dependency.json").exists());
    assert!(!output.join("services-provenance.json").exists());
}
struct LiveCompiler {
    commands: PathBuf,
    missing: bool,
    fail: bool,
    timeout: bool,
}
impl Runner for LiveCompiler {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        let mut spec = spec.clone();
        if spec.argv[0] == "g++" {
            spec.env.insert(
                "PATH".into(),
                if self.missing {
                    self.commands.join("empty").display().to_string()
                } else {
                    format!(
                        "{}:{}",
                        self.commands.display(),
                        std::env::var("PATH").unwrap()
                    )
                },
            );
            spec.env.insert(
                "COMPILER_FIXTURE_EXIT".into(),
                if self.fail { "9" } else { "0" }.into(),
            );
            if self.timeout {
                spec.timeout = std::time::Duration::from_millis(120);
                spec.env
                    .insert("COMPILER_FIXTURE_SLEEP".into(), "30".into());
            }
        }
        openpilot_dev_tools::qa_process::NativeRunner.run(&spec)
    }
}
fn live_compiler_setup(temp: &Path) -> (PathBuf, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let commands = temp.join("commands");
    let toolchain = temp.join("toolchain");
    fs::create_dir_all(commands.join("empty")).unwrap();
    fs::create_dir_all(toolchain.join("bin")).unwrap();
    for (from, to) in [
        ("compiler_interleave.sh", commands.join("g++")),
        ("capnp_fixture.sh", toolchain.join("bin/capnp")),
    ] {
        fs::copy(fixture(from), &to).unwrap();
        fs::set_permissions(to, fs::Permissions::from_mode(0o755)).unwrap();
    }
    (commands, toolchain)
}
#[test]
fn original_real_compiler_stream_order_raw_bytes_and_spawn_failure_are_preserved() {
    let cases: Vec<Value> =
        serde_json::from_slice(&fs::read(fixture("compiler-log-original.json")).unwrap()).unwrap();
    for case in cases {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("root");
        let output = temp.path().join("output");
        setup(&source);
        let (commands, toolchain) = live_compiler_setup(temp.path());
        let args = vec![
            output.to_str().unwrap().into(),
            "--json11-prefix".into(),
            temp.path().join("json11").to_str().unwrap().into(),
            "--capnp-prefix".into(),
            toolchain.to_str().unwrap().into(),
            "--zmq-include".into(),
            temp.path().join("zmq").to_str().unwrap().into(),
        ];
        let mut runner = LiveCompiler {
            commands,
            missing: case["case"] == "spawn-failure",
            fail: case["case"] == "compiler-failure",
            timeout: false,
        };
        let result = logging_oracle_build::run(
            &source,
            "bootlog",
            &args,
            &source.join("rust/target"),
            "x86_64",
            &mut runner,
            u64::MAX,
        );
        assert_eq!(result.is_ok(), case["error"].is_null(), "{}", case["case"]);
        assert!(
            output.join("build.log").is_file(),
            "{} must precreate log",
            case["case"]
        );
        let expected: Vec<u8> = serde_json::from_value(case["log_bytes"].clone()).unwrap();
        assert_eq!(
            fs::read(output.join("build.log")).unwrap(),
            expected,
            "{}",
            case["case"]
        );
        let mut names = fs::read_dir(output)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect::<Vec<_>>();
        names.sort();
        assert_eq!(json!(names), case["top_level"]);
    }
}
#[test]
fn bounded_compiler_timeout_retains_all_partial_merged_diagnostics() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("root");
    let output = temp.path().join("output");
    setup(&source);
    let (commands, toolchain) = live_compiler_setup(temp.path());
    let args = vec![
        output.to_str().unwrap().into(),
        "--json11-prefix".into(),
        temp.path().join("json11").to_str().unwrap().into(),
        "--capnp-prefix".into(),
        toolchain.to_str().unwrap().into(),
        "--zmq-include".into(),
        temp.path().join("zmq").to_str().unwrap().into(),
    ];
    let mut runner = LiveCompiler {
        commands,
        missing: false,
        fail: false,
        timeout: true,
    };
    assert!(logging_oracle_build::run(
        &source,
        "bootlog",
        &args,
        &source.join("rust/target"),
        "x86_64",
        &mut runner,
        u64::MAX
    )
    .is_err());
    let cases: Vec<Value> =
        serde_json::from_slice(&fs::read(fixture("compiler-log-original.json")).unwrap()).unwrap();
    let expected: Vec<u8> = serde_json::from_value(cases[0]["log_bytes"].clone()).unwrap();
    assert_eq!(fs::read(output.join("build.log")).unwrap(), expected);
    assert!(!output.join("source-hashes.json").exists());
}
