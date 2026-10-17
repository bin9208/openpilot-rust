use openpilot_dev_tools::{
    native_logging_build,
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use serde_json::{json, Value};
use std::{
    fs,
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
struct Peer {
    commands: Vec<Vec<String>>,
    fail_compile: bool,
}
impl Runner for Peer {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        self.commands.push(spec.argv.clone());
        let mut code = 0;
        if spec.argv[0] == "curl" {
            fs::copy(
                fixture(spec.argv[2].rsplit('/').next().unwrap()),
                &spec.argv[4],
            )?;
        } else if self.fail_compile {
            code = 9;
        } else {
            fs::write(
                &spec.argv[spec.argv.iter().position(|a| a == "-o").unwrap() + 1],
                b"controlled compiler product\n",
            )?;
        }
        Ok(Captured {
            code,
            stdout: b"controlled stdout\n".to_vec(),
            stderr: b"controlled stderr\n".to_vec(),
        })
    }
}
fn setup(source: &Path) {
    fs::create_dir_all(source).unwrap();
    fs::copy(fixture("json11-lock.toml"), source.join("uv.lock")).unwrap();
    for name in [
        "openpilot/common/swaglog.cc",
        "openpilot/common/swaglog.h",
        "openpilot/common/timing.h",
        "rust/tools/native_logging_reference.cc",
    ] {
        let target = source.join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(root().join(name), target).unwrap();
    }
    fs::create_dir_all(source.join("rust/target/debug/build/zmq-sys-fixture/out/source/include"))
        .unwrap();
}
#[test]
fn both_original_architectures_preserve_stage_clock_overlays_commands_and_provenance() {
    let cases: Vec<Value> =
        serde_json::from_slice(&fs::read(fixture("logging-original.json")).unwrap()).unwrap();
    for case in cases {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("root");
        let output = temp.path().join("output");
        setup(&source);
        let mut peer = Peer {
            commands: vec![],
            fail_compile: false,
        };
        let binary = native_logging_build::build(
            &source,
            &output,
            None,
            case["architecture"].as_str().unwrap(),
            &mut peer,
            u64::MAX,
        )
        .unwrap();
        let normalize = |v: &Value| {
            replace(
                v,
                &[
                    (source.to_str().unwrap(), "$ROOT"),
                    (output.to_str().unwrap(), "$OUTPUT"),
                ],
            )
        };
        assert_eq!(normalize(&json!(binary)), case["binary"]);
        assert_eq!(normalize(&json!(peer.commands)), case["commands"]);
        for (name, expected) in case["files"].as_object().unwrap() {
            let text = fs::read_to_string(output.join(name)).unwrap();
            let value = if name.ends_with(".json") {
                serde_json::from_str(&text).unwrap()
            } else {
                json!(text)
            };
            assert_eq!(normalize(&value), *expected, "{name}");
        }
    }
}
#[test]
fn compiler_failure_keeps_owned_overlay_without_false_success_provenance() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("root");
    let output = temp.path().join("output");
    setup(&source);
    let mut peer = Peer {
        commands: vec![],
        fail_compile: true,
    };
    assert!(
        native_logging_build::build(&source, &output, None, "x86_64", &mut peer, u64::MAX).is_err()
    );
    assert!(output.join("overlay/common/timing.h").is_file());
    assert!(!output.join("provenance.json").exists());
    assert!(!output.join("build.log").exists());
}
#[test]
fn missing_clock_anchor_and_disk_failure_precede_compilation() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("root");
    let output = temp.path().join("output");
    setup(&source);
    let mut peer = Peer {
        commands: vec![],
        fail_compile: false,
    };
    assert!(native_logging_build::build(&source, &output, None, "x86_64", &mut peer, 0).is_err());
    assert!(peer.commands.is_empty());
    fs::write(
        source.join("openpilot/common/timing.h"),
        "missing clock anchor",
    )
    .unwrap();
    assert!(
        native_logging_build::build(&source, &output, None, "x86_64", &mut peer, u64::MAX).is_err()
    );
    assert_eq!(peer.commands.len(), 1);
}
