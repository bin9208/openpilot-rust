use openpilot_dev_tools::{
    json11_stage, panda_runtime_build,
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
    services_header,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
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
    download: Vec<u8>,
    failure: bool,
}
impl Runner for Peer {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        self.commands.push(spec.argv.clone());
        if !self.failure {
            if spec.argv[0] == "curl" {
                fs::write(&spec.argv[4], &self.download)?;
            } else {
                let at = spec.argv.iter().position(|a| a == "-o").unwrap();
                fs::write(&spec.argv[at + 1], b"controlled compiler product\n")?;
            }
        }
        Ok(Captured {
            code: if self.failure { 9 } else { 0 },
            stdout: b"controlled stdout\n".to_vec(),
            stderr: b"controlled stderr\n".to_vec(),
        })
    }
}
#[test]
fn every_service_tuple_order_and_both_header_newlines_match_original() {
    let catalog: Value =
        serde_json::from_slice(&fs::read(fixture("services.json")).unwrap()).unwrap();
    let actual = services_header::catalog();
    assert_eq!(
        actual.as_array().unwrap().len(),
        catalog.as_array().unwrap().len()
    );
    for (actual, expected) in actual
        .as_array()
        .unwrap()
        .iter()
        .zip(catalog.as_array().unwrap())
    {
        // Original navModel decimation is integer-valued 4.0; the Rust catalog
        // stores the same value as u32, as its original generator specifies.
        assert_eq!(
            actual["decimation"].as_f64(),
            expected["decimation"].as_f64()
        );
        let mut actual = actual.clone();
        let mut expected = expected.clone();
        actual.as_object_mut().unwrap().remove("decimation");
        expected.as_object_mut().unwrap().remove("decimation");
        assert_eq!(actual, expected, "service {}", expected["name"]);
    }
    assert_eq!(
        services_header::header(false),
        fs::read_to_string(fixture("services-api.h")).unwrap()
    );
    assert_eq!(
        services_header::header(true),
        fs::read_to_string(fixture("services-script.h")).unwrap()
    );
}
#[test]
fn locked_json11_stages_match_original_for_both_supported_architectures() {
    let cases: Vec<Value> =
        serde_json::from_slice(&fs::read(fixture("json11-original.json")).unwrap()).unwrap();
    for case in cases {
        let temp = tempfile::tempdir().unwrap();
        fs::copy(fixture("json11-lock.toml"), temp.path().join("uv.lock")).unwrap();
        let output = temp.path().join("output");
        let arch = case["architecture"].as_str().unwrap();
        let wheel = format!("json11-fixture-py3-none-manylinux_2_28_{arch}.whl");
        let mut peer = Peer {
            commands: vec![],
            download: fs::read(fixture(&wheel)).unwrap(),
            failure: false,
        };
        let staged = json11_stage::stage(temp.path(), &output, arch, &mut peer, u64::MAX).unwrap();
        assert_eq!(
            staged
                .prefix
                .strip_prefix(&output)
                .unwrap()
                .to_str()
                .unwrap(),
            case["prefix"]
        );
        assert_eq!(staged.dependency, case["dependency"]);
        assert_eq!(
            replace(
                &json!(peer.commands[0]),
                &[(output.to_str().unwrap(), "$OUTPUT")]
            ),
            case["command"]
        );
        for (file, hash) in case["files"].as_object().unwrap() {
            assert_eq!(
                format!("{:x}", Sha256::digest(fs::read(output.join(file)).unwrap())),
                hash.as_str().unwrap()
            );
        }
    }
}
#[test]
fn json11_hash_failure_never_extracts_and_failures_keep_owned_archive() {
    for architecture in ["x86_64", "aarch64"] {
        let temp = tempfile::tempdir().unwrap();
        fs::copy(fixture("json11-lock.toml"), temp.path().join("uv.lock")).unwrap();
        let output = temp.path().join("output");
        let mut peer = Peer {
            commands: vec![],
            download: b"tampered wheel".to_vec(),
            failure: false,
        };
        assert!(
            json11_stage::stage(temp.path(), &output, architecture, &mut peer, u64::MAX).is_err()
        );
        assert!(!output.join("json11").exists());
        assert!(output
            .join(format!(
                "json11-fixture-py3-none-manylinux_2_28_{architecture}.whl"
            ))
            .exists());
        let count = peer.commands.len();
        assert!(
            json11_stage::stage(temp.path(), &output, "unsupported", &mut peer, u64::MAX).is_err()
        );
        assert_eq!(peer.commands.len(), count);
        assert!(json11_stage::stage(temp.path(), &output, architecture, &mut peer, 0).is_err());
        assert_eq!(peer.commands.len(), count);
    }
}

fn setup_runtime(temp: &Path) {
    let generated = temp.join("generated/cereal/gen/cpp");
    fs::create_dir_all(&generated).unwrap();
    for name in ["car", "custom", "deprecated", "log"] {
        fs::write(
            generated.join(format!("{name}.capnp.c++")),
            format!("controlled generated {name}\n"),
        )
        .unwrap();
    }
    fs::create_dir(temp.join("fixture")).unwrap();
    fs::write(
        temp.join("fixture/libusb-1.0.so.0"),
        b"controlled libusb fixture\n",
    )
    .unwrap();
}
#[test]
fn runtime_builder_matches_original_commands_header_manifest_and_sanitizer() {
    let cases: Vec<Value> =
        serde_json::from_slice(&fs::read(fixture("runtime-original.json")).unwrap()).unwrap();
    let root = root();
    for case in cases {
        let temp = tempfile::tempdir().unwrap();
        setup_runtime(temp.path());
        let output = temp.path().join("output");
        let args: Vec<String> = serde_json::from_value(replace(
            &case["args"],
            &[
                ("$OUTPUT", output.to_str().unwrap()),
                ("$ROOT", root.to_str().unwrap()),
                ("$DEPS", temp.path().to_str().unwrap()),
            ],
        ))
        .unwrap();
        let mut peer = Peer {
            commands: vec![],
            download: vec![],
            failure: false,
        };
        panda_runtime_build::run(&root, &args, &mut peer, u64::MAX).unwrap();
        let normalize = |v: &Value| {
            replace(
                v,
                &[
                    (output.to_str().unwrap(), "$OUTPUT"),
                    (root.to_str().unwrap(), "$ROOT"),
                    (temp.path().to_str().unwrap(), "$DEPS"),
                ],
            )
        };
        assert_eq!(normalize(&json!(peer.commands)), case["commands"]);
        assert_eq!(
            fs::read_to_string(output.join("cereal/services.h")).unwrap(),
            case["header"]
        );
        for (name, expected) in case["files"].as_object().unwrap() {
            let mut actual: Value =
                serde_json::from_slice(&fs::read(output.join(name)).unwrap()).unwrap();
            if name == "manifest.json" {
                let provenance = actual
                    .as_object_mut()
                    .unwrap()
                    .remove("native_generator")
                    .expect("native generator provenance required");
                assert_eq!(
                    provenance["header_sha256"],
                    format!(
                        "{:x}",
                        Sha256::digest(fs::read(output.join("cereal/services.h")).unwrap())
                    )
                );
                for source in [
                    "rust/crates/dev-tools/src/services_header.rs",
                    "rust/crates/messaging/src/services_generated.rs",
                ] {
                    assert_eq!(
                        provenance["source_sha256"][source],
                        format!("{:x}", Sha256::digest(fs::read(root.join(source)).unwrap()))
                    );
                }
            }
            assert_eq!(normalize(&actual), *expected);
        }
    }
}
#[test]
fn runtime_failure_preserves_header_command_log_and_rejects_wrong_generated_count() {
    let cases: Vec<Value> =
        serde_json::from_slice(&fs::read(fixture("runtime-original.json")).unwrap()).unwrap();
    let root = root();
    let temp = tempfile::tempdir().unwrap();
    setup_runtime(temp.path());
    let output = temp.path().join("output");
    let args: Vec<String> = serde_json::from_value(replace(
        &cases[0]["args"],
        &[
            ("$OUTPUT", output.to_str().unwrap()),
            ("$ROOT", root.to_str().unwrap()),
            ("$DEPS", temp.path().to_str().unwrap()),
        ],
    ))
    .unwrap();
    let mut peer = Peer {
        commands: vec![],
        download: vec![],
        failure: true,
    };
    assert!(panda_runtime_build::run(&root, &args, &mut peer, u64::MAX).is_err());
    assert!(output.join("command.json").is_file());
    assert!(output.join("cereal/services.h").is_file());
    assert!(!output.join("manifest.json").exists());
    fs::remove_dir_all(&output).unwrap();
    fs::remove_file(temp.path().join("generated/cereal/gen/cpp/car.capnp.c++")).unwrap();
    let count = peer.commands.len();
    assert!(panda_runtime_build::run(&root, &args, &mut peer, u64::MAX).is_err());
    assert_eq!(peer.commands.len(), count);
}
#[test]
fn malformed_json11_lock_structures_return_errors_without_panics() {
    for (index, lock) in [
        "",
        "package = []",
        "[[package]]\nname='other'",
        "[[package]]\nname='comma-deps-json11'\nversion='fixture'",
        "[[package]]\nname='comma-deps-json11'\nversion='fixture'\nwheels=[{}]",
    ]
    .iter()
    .enumerate()
    {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("uv.lock"), lock).unwrap();
        let mut peer = Peer {
            commands: vec![],
            download: vec![],
            failure: false,
        };
        assert!(
            json11_stage::stage(
                temp.path(),
                &temp.path().join("output"),
                "x86_64",
                &mut peer,
                u64::MAX
            )
            .is_err(),
            "case {index}"
        );
        assert!(peer.commands.is_empty());
    }
}
