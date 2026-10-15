use openpilot_dev_tools::{
    encoder_source_build,
    qa_process::{CaptureError, Captured, CommandSpec, Runner},
    seal::Result,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    time::Duration,
};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap()
}
fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/encoder-source")
}
fn setup(source: &Path) {
    fs::create_dir_all(source).unwrap();
    for name in ["openpilot", "msgq_repo", "opendbc_repo"] {
        symlink(root().join(name), source.join(name)).unwrap();
    }
}
fn replace(v: &Value, pairs: &[(&str, &str)]) -> Value {
    match v {
        Value::String(s) => {
            Value::String(pairs.iter().fold(s.clone(), |s, (a, b)| s.replace(a, b)))
        }
        Value::Array(a) => Value::Array(a.iter().map(|v| replace(v, pairs)).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, v)| {
                    (
                        replace(&json!(k), pairs).as_str().unwrap().into(),
                        replace(v, pairs),
                    )
                })
                .collect(),
        ),
        _ => v.clone(),
    }
}
const LOG: &[u8] = b"out-1\nerr-1\nout-2\nerr-2\n\xff\x00raw\n";
struct Peer {
    case: String,
    calls: Vec<CommandSpec>,
}
impl Runner for Peer {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        self.calls.push(spec.clone());
        if spec.argv[0] == "capnp" {
            if self.case == "capnp-failure" {
                return Ok(Captured {
                    code: 8,
                    stdout: vec![],
                    stderr: vec![],
                });
            }
            let generated = PathBuf::from(
                spec.argv
                    .iter()
                    .find_map(|a| a.strip_prefix("-oc++:"))
                    .unwrap(),
            );
            for arg in &spec.argv {
                if arg.ends_with(".capnp") {
                    let p = Path::new(arg);
                    fs::write(
                        generated.join(format!("{}.c++", p.file_name().unwrap().to_str().unwrap())),
                        format!(
                            "controlled generated {}\n",
                            p.file_stem().unwrap().to_str().unwrap()
                        ),
                    )?;
                }
            }
            if self.case == "services-path-collision" {
                fs::create_dir(
                    generated
                        .parent()
                        .unwrap()
                        .parent()
                        .unwrap()
                        .join("services.h"),
                )?;
            }
            return Ok(Captured {
                code: 0,
                stdout: vec![],
                stderr: vec![],
            });
        }
        if self.case == "compiler-spawn-failure" {
            return Err(std::io::Error::from(std::io::ErrorKind::NotFound).into());
        }
        if self.case == "compiler-timeout" {
            return Err(CaptureError::Timeout {
                timeout: Duration::from_secs(180),
                stdout: LOG.to_vec(),
                stderr: vec![],
            }
            .into());
        }
        let code = if self.case == "compiler-failure" {
            9
        } else {
            0
        };
        if code == 0 && self.case != "missing-binary" {
            fs::write(spec.argv.last().unwrap(), b"controlled compiler product\n")?;
        }
        Ok(Captured {
            code,
            stdout: LOG.to_vec(),
            stderr: vec![],
        })
    }
}
fn files(
    path: &Path,
    base: &Path,
    pairs: &[(&str, &str)],
    result: &mut serde_json::Map<String, Value>,
) {
    if !path.exists() {
        return;
    }
    for entry in fs::read_dir(path).unwrap() {
        let p = entry.unwrap().path();
        if p.is_dir() {
            files(&p, base, pairs, result);
        } else {
            let bytes = fs::read(&p).unwrap();
            let value = if p.extension().is_some_and(|e| e == "json") {
                replace(&serde_json::from_slice(&bytes).unwrap(), pairs)
            } else if p.file_name().unwrap() == "build.log" {
                json!({"sha256":format!("{:x}",Sha256::digest(&bytes)),"bytes":bytes})
            } else {
                json!(format!("{:x}", Sha256::digest(&bytes)))
            };
            result.insert(
                p.strip_prefix(base).unwrap().to_str().unwrap().into(),
                value,
            );
        }
    }
}
// Only original filesystem-enumerated argument slots may change order across hosts.
fn canonical_globs(value: &Value) -> Value {
    let mut args = value.as_array().unwrap().clone();
    for suffix in [".capnp", ".capnp.c++"] {
        let positions = args
            .iter()
            .enumerate()
            .filter(|(_, v)| v.as_str().is_some_and(|s| s.ends_with(suffix)))
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        let mut values = positions
            .iter()
            .map(|i| args[*i].clone())
            .collect::<Vec<_>>();
        values.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
        for (position, value) in positions.into_iter().zip(values) {
            args[position] = value;
        }
    }
    Value::Array(args)
}
fn assert_live_glob(call: &CommandSpec, output: &Path) {
    let (directory, suffix, extension) = if call.argv[0] == "capnp" {
        (output.join("schema"), ".capnp", "capnp")
    } else {
        (output.join("cereal/gen/cpp"), ".capnp.c++", "c++")
    };
    let live = fs::read_dir(directory)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == extension))
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>();
    let actual = call
        .argv
        .iter()
        .filter(|s| s.ends_with(suffix))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(actual, live);
}
#[test]
fn original_success_prefix_resolution_and_failure_artifact_contracts() {
    let cases: Vec<Value> =
        serde_json::from_slice(&fs::read(fixtures().join("original.json")).unwrap()).unwrap();
    for case in cases {
        if case["case"] == "services-failure" || case["case"] == "abbreviated-help" {
            continue;
        } // Python generator failures disappear with the compiled catalog.
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("root");
        let output = tmp.path().join("output");
        let deps = tmp.path().join("deps");
        setup(&source);
        if case["case"] == "output-collision" {
            fs::create_dir(&output).unwrap();
            fs::write(output.join("owned-marker"), b"original marker").unwrap();
        }
        if case["case"] == "resolved-prefixes" {
            fs::create_dir_all(deps.join("linked/subdir")).unwrap();
            symlink(deps.join("linked/subdir"), deps.join("link")).unwrap();
        }
        let real = [
            ("$ROOT", source.to_str().unwrap()),
            ("$OUTPUT", output.to_str().unwrap()),
            ("$DEPS", deps.to_str().unwrap()),
        ];
        let args: Vec<String> = serde_json::from_value(replace(&case["args"], &real)).unwrap();
        let mut peer = Peer {
            case: case["case"].as_str().unwrap().into(),
            calls: vec![],
        };
        let result = encoder_source_build::run(
            &source,
            &args,
            &mut peer,
            case["free_bytes"].as_u64().unwrap(),
        );
        assert_eq!(
            result.is_err(),
            !case["error"].is_null(),
            "{}",
            case["case"]
        );
        if case["case"] == "services-path-collision" {
            assert!(output.join("cereal/services.h").is_dir());
        }
        let normalize = [
            (source.to_str().unwrap(), "$ROOT"),
            (output.to_str().unwrap(), "$OUTPUT"),
            (deps.to_str().unwrap(), "$DEPS"),
        ];
        let expected: Vec<&Value> = case["commands"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["argv"][0] != "python3")
            .collect();
        assert_eq!(peer.calls.len(), expected.len(), "{}", case["case"]);
        for (call, expected) in peer.calls.iter().zip(expected) {
            assert_eq!(
                canonical_globs(&replace(&json!(call.argv), &normalize)),
                canonical_globs(&expected["argv"]),
                "{}",
                case["case"]
            );
            assert_live_glob(call, &output);
            if call.argv[0] == "g++" {
                assert_eq!(call.timeout, Duration::from_secs(180));
                assert!(call.merge_output);
            } else {
                assert!(call.inherit_output);
                assert_eq!(call.timeout, Duration::MAX);
            }
        }
        let mut actual = serde_json::Map::new();
        files(&output, &output, &normalize, &mut actual);
        actual.remove("services-provenance.json");
        let mut expected_files = case["files"].clone();
        if let Some(command) = actual.get_mut("build-command.json") {
            *command = canonical_globs(command);
        }
        if let Some(command) = expected_files.get_mut("build-command.json") {
            *command = canonical_globs(command);
        }
        assert_eq!(Value::Object(actual), expected_files, "{}", case["case"]);
    }
}
#[test]
fn all_required_flags_are_checked_before_output_creation() {
    let tmp = tempfile::tempdir().unwrap();
    let mut peer = Peer {
        case: "success".into(),
        calls: vec![],
    };
    for args in [
        vec![],
        vec!["--output".into()],
        vec!["--unknown".into(), "x".into()],
    ] {
        assert!(encoder_source_build::run(tmp.path(), &args, &mut peer, u64::MAX).is_err());
    }
    assert!(peer.calls.is_empty());
}
fn compiler_setup(temp: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let commands = temp.join("commands");
    fs::create_dir(&commands).unwrap();
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/native-build-closure");
    for (from, to) in [
        ("capnp_fixture.sh", "capnp"),
        ("compiler_interleave.sh", "g++"),
    ] {
        let dest = commands.join(to);
        fs::copy(base.join(from), &dest).unwrap();
        fs::set_permissions(dest, fs::Permissions::from_mode(0o755)).unwrap();
    }
    commands
}
fn args(output: &Path, deps: &Path) -> Vec<String> {
    let mut args = vec!["--output".into(), output.display().to_string()];
    for name in [
        "native-prefix",
        "capnp-prefix",
        "json11-prefix",
        "zmq-include",
        "jpeg-build",
    ] {
        args.extend([format!("--{name}"), deps.join(name).display().to_string()]);
    }
    args
}
#[test]
fn real_cli_matches_original_raw_merged_log_spawn_and_nonzero_failures() {
    use std::process::Command;
    let cases: Vec<Value> =
        serde_json::from_slice(&fs::read(fixtures().join("streams-original.json")).unwrap())
            .unwrap();
    for case in cases {
        let temp = tempfile::tempdir().unwrap();
        let commands = compiler_setup(temp.path());
        if case["case"] == "compiler-spawn-failure" {
            fs::remove_file(commands.join("g++")).unwrap();
        }
        let output = temp.path().join("output");
        let captured = Command::new(env!("CARGO_BIN_EXE_build-encoder-source"))
            .args(args(&output, temp.path()))
            .env("PATH", &commands)
            .env(
                "COMPILER_FIXTURE_EXIT",
                if case["case"] == "compiler-failure" {
                    "9"
                } else {
                    "0"
                },
            )
            .output()
            .unwrap();
        assert_eq!(
            captured.status.code(),
            Some(case["code"].as_i64().unwrap() as i32),
            "{}",
            case["case"]
        );
        let bytes: Vec<u8> = serde_json::from_value(case["log_bytes"].clone()).unwrap();
        assert_eq!(fs::read(output.join("build.log")).unwrap(), bytes);
        assert_eq!(
            output.join("receipt.json").is_file(),
            case["receipt_exists"].as_bool().unwrap()
        );
        if captured.status.success() {
            assert_eq!(
                captured.stdout,
                format!("{}\n", output.join("original-encoderd").display()).as_bytes()
            );
        }
    }
}
struct TimeoutPeer {
    commands: PathBuf,
}
impl TimeoutPeer {
    fn command(&self, spec: &CommandSpec) -> CommandSpec {
        let mut spec = spec.clone();
        spec.env
            .insert("PATH".into(), format!("{}:/bin", self.commands.display()));
        if spec.argv[0] == "g++" {
            spec.timeout = Duration::from_millis(120);
            spec.env
                .insert("COMPILER_FIXTURE_SLEEP".into(), "30".into());
        }
        spec
    }
}
impl Runner for TimeoutPeer {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        openpilot_dev_tools::qa_process::NativeRunner.run(&self.command(spec))
    }
    fn run_to_file(&mut self, spec: &CommandSpec, log: &Path) -> Result<Captured> {
        openpilot_dev_tools::qa_process::NativeRunner.run_to_file(&self.command(spec), log)
    }
}
#[test]
fn actual_owned_compiler_timeout_preserves_partial_log_without_success_receipt() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("root");
    setup(&source);
    let output = temp.path().join("output");
    let mut peer = TimeoutPeer {
        commands: compiler_setup(temp.path()),
    };
    assert!(
        encoder_source_build::run(&source, &args(&output, temp.path()), &mut peer, u64::MAX)
            .is_err()
    );
    assert_eq!(fs::read(output.join("build.log")).unwrap(), LOG);
    assert!(!output.join("receipt.json").exists());
}
#[test]
fn original_argparse_help_abbreviation_and_missing_value_precedence() {
    use std::process::Command;
    let cases: Vec<Value> =
        serde_json::from_slice(&fs::read(fixtures().join("original.json")).unwrap()).unwrap();
    for case in cases.into_iter().filter(|c| {
        c["case"] == "abbreviated-help"
            || c["case"] == "help-after-missing-value"
            || c["case"] == "ambiguous-abbreviation"
            || c["case"] == "ambiguous-equals"
    }) {
        let args: Vec<String> = serde_json::from_value(case["args"].clone()).unwrap();
        let captured = Command::new(env!("CARGO_BIN_EXE_build-encoder-source"))
            .args(args)
            .output()
            .unwrap();
        assert_eq!(
            captured.status.code(),
            Some(case["exit_code"].as_i64().unwrap() as i32),
            "{}",
            case["case"]
        );
        if captured.status.success() {
            let help = String::from_utf8(captured.stdout).unwrap();
            for flag in [
                "--output",
                "--native-prefix",
                "--capnp-prefix",
                "--json11-prefix",
                "--zmq-include",
                "--jpeg-build",
            ] {
                assert!(help.contains(flag), "{flag}");
            }
        }
    }
}
