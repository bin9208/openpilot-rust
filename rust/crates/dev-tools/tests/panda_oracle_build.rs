use openpilot_dev_tools::{
    panda_oracle_build,
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

struct Peer {
    commands: Vec<Vec<String>>,
    fail: Option<usize>,
}
impl Runner for Peer {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        self.commands.push(spec.argv.clone());
        let code = if self.fail == Some(self.commands.len()) {
            9
        } else {
            0
        };
        if code == 0 {
            if spec.argv[0] == "capnp" {
                let generated = spec
                    .argv
                    .iter()
                    .find_map(|a| a.strip_prefix("-oc++:"))
                    .unwrap();
                for name in ["car", "custom", "deprecated", "log"] {
                    fs::write(
                        Path::new(generated).join(format!("{name}.capnp.c++")),
                        format!("controlled generated {name}\n"),
                    )?;
                }
            } else {
                let at = spec.argv.iter().position(|v| v == "-o").unwrap();
                fs::write(&spec.argv[at + 1], b"controlled compiler product\n")?;
            }
        }
        Ok(Captured {
            code,
            stdout: b"controlled stdout\n".to_vec(),
            stderr: b"controlled stderr\n".to_vec(),
        })
    }
}
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap()
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
                        replace(&Value::String(k.clone()), pairs)
                            .as_str()
                            .unwrap()
                            .to_owned(),
                        replace(v, pairs),
                    )
                })
                .collect(),
        ),
        _ => value.clone(),
    }
}
#[test]
fn original_commands_includes_and_manifests_match_all_seventeen_cases() {
    let root = root();
    const HISTORICAL_DEVICE_ORACLE: &str =
        "dbf7b2de42a945047f3c0d8f5450f3e89e16452cb454c4b922ee81b799347468";
    const CURRENT_DEVICE_ORACLE: &str =
        "96a8b74bf789bf96a91067bf4a174aaca3bbd455133eb4c48bdd8a6598a09197";
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(fs::read(root.join("rust/tools/pandad_device_source.cc")).unwrap())
        ),
        CURRENT_DEVICE_ORACLE
    );
    let root_s = root.to_str().unwrap();
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/panda-oracle/original.json")).unwrap();
    for case in cases {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path();
        let output = base.join("output");
        fs::create_dir_all(base.join("usb-headers/libusb-1.0")).unwrap();
        fs::write(
            base.join("usb-headers/libusb-1.0/libusb.h"),
            b"controlled libusb header\n",
        )
        .unwrap();
        let base_s = base.to_str().unwrap();
        let out_s = output.to_str().unwrap();
        let expanded = replace(
            &case["args"],
            &[("$OUTPUT", out_s), ("$ROOT", root_s), ("$DEPS", base_s)],
        );
        let args: Vec<String> = serde_json::from_value(expanded).unwrap();
        let label = case["case"].as_str().unwrap();
        let family = if label.starts_with("protocol-") {
            "protocol"
        } else if label.starts_with("runtime-fixture") {
            "runtime-fixture"
        } else {
            label.split('-').next().unwrap()
        };
        let mut peer = Peer {
            commands: vec![],
            fail: None,
        };
        panda_oracle_build::run(&root, family, &args, &mut peer, u64::MAX).unwrap();
        let normalize = |v: &Value| {
            replace(
                v,
                &[(out_s, "$OUTPUT"), (root_s, "$ROOT"), (base_s, "$DEPS")],
            )
        };
        assert_eq!(
            normalize(&serde_json::to_value(&peer.commands).unwrap()),
            case["commands"],
            "{label}"
        );
        for (name, expected) in case["files"].as_object().unwrap() {
            let content = fs::read_to_string(output.join(name)).unwrap();
            let actual = if name.ends_with(".json") {
                serde_json::from_str(&content).unwrap()
            } else {
                Value::String(content)
            };
            let mut historical = expected.clone();
            if matches!(label, "protocol-device" | "protocol-device-sanitize")
                && name == "manifest.json"
            {
                let pinned = &mut historical["sources"]["rust/tools/pandad_device_source.cc"];
                assert_eq!(
                    pinned.as_str(),
                    Some(HISTORICAL_DEVICE_ORACLE),
                    "historical {label}"
                );
                *pinned = CURRENT_DEVICE_ORACLE.into();
            } else {
                assert!(
                    !historical.to_string().contains(HISTORICAL_DEVICE_ORACLE),
                    "unexpected historical device hash in {label}: {name}"
                );
            }
            assert_eq!(normalize(&actual), historical, "{label}: {name}");
        }
    }
}
#[test]
fn failures_keep_owned_output_and_logs_without_success_manifest() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("output");
    let args = vec![
        "--output".into(),
        output.to_str().unwrap().into(),
        "--json11-prefix".into(),
        "/unused".into(),
    ];
    let mut peer = Peer {
        commands: vec![],
        fail: Some(1),
    };
    assert!(panda_oracle_build::run(&root(), "spi", &args, &mut peer, u64::MAX).is_err());
    assert!(output.join("command.json").is_file());
    assert!(!output.join("manifest.json").exists());
    assert!(panda_oracle_build::run(&root(), "spi", &args, &mut peer, u64::MAX).is_err());
    assert_eq!(peer.commands.len(), 1);
}
#[test]
fn disk_admission_precedes_commands_and_source_anchor_failure_is_explicit() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("output");
    let args = vec![
        "--output".into(),
        output.to_str().unwrap().into(),
        "--json11-prefix".into(),
        "/unused".into(),
    ];
    let mut peer = Peer {
        commands: vec![],
        fail: None,
    };
    assert!(panda_oracle_build::run(&root(), "peripheral", &args, &mut peer, 0).is_err());
    assert!(peer.commands.is_empty());
    assert!(output.is_dir());
    fs::remove_dir(&output).unwrap();
    let fake = temp.path().join("root");
    fs::create_dir_all(fake.join("openpilot/selfdrive/pandad")).unwrap();
    fs::write(
        fake.join("openpilot/selfdrive/pandad/pandad.cc"),
        "missing anchors",
    )
    .unwrap();
    let error =
        panda_oracle_build::run(&fake, "peripheral", &args, &mut peer, u64::MAX).unwrap_err();
    assert!(error.to_string().contains("process_peripheral_state"));
    assert!(peer.commands.is_empty());
}
#[test]
fn cli_rejects_inapplicable_flags_conflicting_modes_and_unknown_families() {
    for (family, tail) in [
        ("protocol", vec!["--state", "--safety"]),
        ("spi", vec!["--device"]),
        ("runtime-fixture", vec!["--sanitize"]),
        ("usb", vec!["--compiler"]),
        ("unknown", vec![]),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("output");
        let mut args = vec![
            "--output".into(),
            output.to_str().unwrap().into(),
            "--json11-prefix".into(),
            "/unused".into(),
        ];
        args.extend(tail.into_iter().map(str::to_owned));
        let mut peer = Peer {
            commands: vec![],
            fail: None,
        };
        assert!(panda_oracle_build::run(&root(), family, &args, &mut peer, u64::MAX).is_err());
        assert!(!output.exists());
        assert!(peer.commands.is_empty());
    }
}
#[test]
fn second_usb_compile_failure_keeps_first_product_and_both_command_records() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("output");
    let args = vec![
        "--output".into(),
        output.to_str().unwrap().into(),
        "--json11-prefix".into(),
        "/unused".into(),
        "--libusb-include".into(),
        "/unused".into(),
    ];
    let mut peer = Peer {
        commands: vec![],
        fail: Some(2),
    };
    assert!(panda_oracle_build::run(&root(), "usb", &args, &mut peer, u64::MAX).is_err());
    assert!(output.join("pandad-usb-source").is_file());
    assert!(!output.join("manifest.json").exists());
    let commands: Vec<Value> =
        serde_json::from_slice(&fs::read(output.join("command.json")).unwrap()).unwrap();
    assert_eq!(commands.len(), 2);
    assert_eq!(commands[1]["exit_code"], 9);
}
#[test]
fn original_failure_artifacts_match_each_builder_and_both_protocol_stages() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/panda-oracle/original-failures.json")).unwrap();
    for case in cases {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("output");
        let family = case["family"].as_str().unwrap().replace('_', "-");
        let mut args = vec![
            "--output".into(),
            output.to_str().unwrap().into(),
            "--json11-prefix".into(),
            "/unused".into(),
        ];
        if family == "protocol" {
            args.extend(["--capnp-prefix".into(), "/unused".into()]);
        }
        if family == "usb" || family == "runtime-fixture" {
            args.extend(["--libusb-include".into(), "/unused".into()]);
        }
        let mut peer = Peer {
            commands: vec![],
            fail: Some(case["failed_command"].as_u64().unwrap() as usize),
        };
        assert!(panda_oracle_build::run(&root(), &family, &args, &mut peer, u64::MAX).is_err());
        let mut files: Vec<String> = fs::read_dir(&output)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        files.sort();
        assert_eq!(
            serde_json::to_value(files).unwrap(),
            case["files"],
            "{family}"
        );
        assert_eq!(
            peer.commands.len(),
            case["command_count"].as_u64().unwrap() as usize
        );
    }
}
#[test]
fn usb_and_spi_default_to_original_clang_compiler() {
    for family in ["usb", "spi"] {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("output");
        let mut args = vec![
            "--output".into(),
            output.to_str().unwrap().into(),
            "--json11-prefix".into(),
            "/unused".into(),
        ];
        if family == "usb" {
            args.extend(["--libusb-include".into(), "/unused".into()]);
        }
        let mut peer = Peer {
            commands: vec![],
            fail: Some(1),
        };
        assert!(panda_oracle_build::run(&root(), family, &args, &mut peer, u64::MAX).is_err());
        assert_eq!(peer.commands[0][0], "clang++");
    }
}
#[test]
fn runtime_fixture_preserves_original_custom_compiler_and_documents_flag() {
    let original: Value = serde_json::from_str(include_str!(
        "fixtures/panda-oracle/original-runtime-compiler.json"
    ))
    .unwrap();
    let root = root();
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("output");
    fs::create_dir_all(temp.path().join("usb/libusb-1.0")).unwrap();
    fs::write(
        temp.path().join("usb/libusb-1.0/libusb.h"),
        b"controlled libusb header\n",
    )
    .unwrap();
    let pairs = [
        ("$OUTPUT", output.to_str().unwrap()),
        ("$ROOT", root.to_str().unwrap()),
        ("$DEPS", temp.path().to_str().unwrap()),
    ];
    let args: Vec<String> = serde_json::from_value(replace(&original["args"], &pairs)).unwrap();
    let mut peer = Peer {
        commands: vec![],
        fail: None,
    };
    panda_oracle_build::run(&root, "runtime-fixture", &args, &mut peer, u64::MAX).unwrap();
    let normalize = [
        (output.to_str().unwrap(), "$OUTPUT"),
        (root.to_str().unwrap(), "$ROOT"),
        (temp.path().to_str().unwrap(), "$DEPS"),
    ];
    assert_eq!(
        replace(&serde_json::to_value(&peer.commands).unwrap(), &normalize),
        original["commands"]
    );
    for (name, value) in original["files"].as_object().unwrap() {
        let actual: Value = serde_json::from_slice(&fs::read(output.join(name)).unwrap()).unwrap();
        assert_eq!(replace(&actual, &normalize), *value);
    }
    let help = std::process::Command::new(env!("CARGO_BIN_EXE_build-pandad-runtime-fixture"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("--compiler COMPILER"));
}
