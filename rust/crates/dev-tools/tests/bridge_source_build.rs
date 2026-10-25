use openpilot_dev_tools::{
    bridge_source_build,
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};
fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bridge-builder/original.json")
}
struct Peer {
    calls: Vec<CommandSpec>,
    fail: bool,
}
impl Runner for Peer {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        self.calls.push(spec.clone());
        if !self.fail {
            fs::write(spec.argv.last().unwrap(), b"compiler product")?;
        }
        Ok(Captured {
            code: if self.fail { 9 } else { 0 },
            stdout: vec![],
            stderr: vec![],
        })
    }
}
fn setup(target: &Path) {
    for (name, valid) in [
        ("zmq-sys-a", false),
        ("zmq-sys-z", true),
        ("zmq-sys-b", true),
    ] {
        let p = target.join("debug/build").join(name).join("out");
        fs::create_dir_all(p.join("lib")).unwrap();
        fs::write(p.join("lib/libzmq.a"), b"lib").unwrap();
        if valid {
            fs::create_dir_all(p.join("source/include")).unwrap();
            fs::write(p.join("source/include/zmq.h"), b"header").unwrap();
        }
    }
}
fn norm(s: &str, root: &Path, out: &Path, target: &Path) -> String {
    s.replace(target.to_str().unwrap(), "$TARGET")
        .replace(root.to_str().unwrap(), "$ROOT")
        .replace(out.to_str().unwrap(), "$OUTPUT")
}
#[test]
fn original_explicit_discovery_failure_and_header_contracts() {
    let cases: Vec<Value> = serde_json::from_slice(&fs::read(fixture()).unwrap()).unwrap();
    for case in cases {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        let output = tmp.path().join("output");
        let target = if case["case"] == "default-target" {
            root.join("rust/target")
        } else {
            tmp.path().join("target")
        };
        setup(&target);
        if case["case"] == "missing" {
            fs::remove_dir_all(&target).unwrap();
        }
        let args: Vec<String> = case["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                v.as_str()
                    .unwrap()
                    .replace("$ROOT", root.to_str().unwrap())
                    .replace("$OUTPUT", output.to_str().unwrap())
                    .replace("$TARGET", target.to_str().unwrap())
            })
            .collect();
        let mut peer = Peer {
            calls: vec![],
            fail: case["case"] == "compiler-failure",
        };
        let result = bridge_source_build::run(&root, &args, &mut peer, u64::MAX);
        assert_eq!(
            result.is_err(),
            !case["error"].is_null(),
            "{}",
            case["case"]
        );
        assert_eq!(output.exists(), case["output_exists"].as_bool().unwrap());
        let original: Vec<&Value> = case["calls"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["argv"][0] == "g++")
            .collect();
        assert_eq!(peer.calls.len(), original.len());
        for (call, original) in peer.calls.iter().zip(original) {
            assert_eq!(
                call.argv
                    .iter()
                    .map(|s| norm(s, &root, &output, &target))
                    .collect::<Vec<_>>(),
                serde_json::from_value::<Vec<String>>(original["argv"].clone()).unwrap()
            );
            assert_eq!(call.timeout.as_secs(), 120);
            assert!(call.inherit_output);
        }
        for (name, digest) in case["files"].as_object().unwrap() {
            assert_eq!(
                format!("{:x}", Sha256::digest(fs::read(output.join(name)).unwrap())),
                digest.as_str().unwrap()
            );
        }
    }
}
#[test]
fn low_disk_and_invalid_arguments_do_not_start_compilers() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("target");
    setup(&target);
    let output = tmp.path().join("output");
    let mut peer = Peer {
        calls: vec![],
        fail: false,
    };
    let args = vec![
        "--output".into(),
        output.display().to_string(),
        "--target".into(),
        target.display().to_string(),
    ];
    assert!(bridge_source_build::run(tmp.path(), &args, &mut peer, 0).is_err());
    assert!(!output.exists());
    assert!(peer.calls.is_empty());
    for args in [
        vec![],
        vec!["--output".into()],
        vec!["--unknown".into(), "x".into()],
    ] {
        assert!(bridge_source_build::run(tmp.path(), &args, &mut peer, u64::MAX).is_err());
    }
    assert!(peer.calls.is_empty());
}
#[test]
fn real_cli_preserves_original_inherited_raw_streams_and_partial_files() {
    use std::{os::unix::fs::PermissionsExt, process::Command};
    let fixtures = fixture().parent().unwrap().to_path_buf();
    let cases: Vec<Value> =
        serde_json::from_slice(&fs::read(fixtures.join("streams-original.json")).unwrap()).unwrap();
    for case in cases {
        let temp = tempfile::tempdir().unwrap();
        let commands = temp.path().join("commands");
        fs::create_dir(&commands).unwrap();
        if case["case"] != "spawn-failure" {
            let script = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/native-build-closure/compiler_interleave.sh");
            let compiler = commands.join("g++");
            fs::copy(script, &compiler).unwrap();
            fs::set_permissions(compiler, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let output = temp.path().join("out");
        let captured = Command::new(env!("CARGO_BIN_EXE_build-bridge-source"))
            .args([
                "--output",
                output.to_str().unwrap(),
                "--zmq-root",
                temp.path().join("zmq").to_str().unwrap(),
            ])
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
            captured.status.success(),
            case["code"] == 0,
            "{}",
            case["case"]
        );
        let stdout: Vec<u8> = serde_json::from_value(case["stdout"].clone()).unwrap();
        let stderr: Vec<u8> = serde_json::from_value(case["stderr"].clone()).unwrap();
        assert_eq!(captured.stdout, stdout, "{}", case["case"]);
        assert!(
            captured.stderr.starts_with(&stderr),
            "{}: {:?}",
            case["case"],
            captured.stderr
        );
        if captured.status.success() {
            assert_eq!(captured.stderr, stderr);
        }
        assert_eq!(
            output.join("include/cereal/services.h").is_file(),
            case["header_exists"].as_bool().unwrap()
        );
        assert_eq!(
            output.join("bridge-original").is_file(),
            case["binary_exists"].as_bool().unwrap()
        );
    }
}
#[test]
fn missing_zmq_cache_preserves_original_parser_error_without_output_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("output");
    let target = temp.path().join("missing-target");
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_build-bridge-source"))
        .arg("--output")
        .arg(&output)
        .arg("--target")
        .arg(&target)
        .output()
        .unwrap();
    assert_eq!(
        result.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stderr)
        .contains("build openpilot-bridge first or provide --zmq-root"));
    assert!(!output.exists());
    assert!(!target.exists());
}
