use openpilot_dev_tools::{
    pandad_firmware_spi_build,
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap()
}
fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pandad-firmware-spi")
}
fn replace(v: &Value, root: &Path, cwd: &Path) -> Value {
    match v {
        Value::String(s) => json!(s
            .replace("$ROOT", root.to_str().unwrap())
            .replace("$CWD", cwd.to_str().unwrap())),
        Value::Array(a) => Value::Array(a.iter().map(|v| replace(v, root, cwd)).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, v)| {
                    (
                        replace(&json!(k), root, cwd).as_str().unwrap().into(),
                        replace(v, root, cwd),
                    )
                })
                .collect(),
        ),
        _ => v.clone(),
    }
}
fn files(path: &Path, base: &Path, result: &mut serde_json::Map<String, Value>) {
    if !path.exists() {
        return;
    }
    for entry in fs::read_dir(path).unwrap() {
        let p = entry.unwrap().path();
        if p.is_dir() {
            files(&p, base, result);
        } else {
            let bytes = fs::read(&p).unwrap();
            result.insert(
                p.strip_prefix(base).unwrap().display().to_string(),
                if p.extension().is_some_and(|e| e == "json") {
                    serde_json::from_slice(&bytes).unwrap()
                } else {
                    json!(format!("{:x}", Sha256::digest(bytes)))
                },
            );
        }
    }
}
#[test]
fn original_compiler_text_failure_manifest_path_and_argparse_cases() {
    let cases: Vec<Value> =
        serde_json::from_slice(&fs::read(fixtures().join("original.json")).unwrap()).unwrap();
    for case in cases {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("root");
        let cwd = tmp.path().join("cwd");
        fs::create_dir_all(source.join("rust/tools")).unwrap();
        fs::create_dir(&cwd).unwrap();
        symlink(
            root().join("rust/tools/pandad_firmware_spidev.cc"),
            source.join("rust/tools/pandad_firmware_spidev.cc"),
        )
        .unwrap();
        let name = case["case"].as_str().unwrap();
        if name == "symlink-output" {
            fs::create_dir_all(cwd.join("real/subdir")).unwrap();
            symlink(cwd.join("real/subdir"), cwd.join("link")).unwrap();
        }
        let spidev = PathBuf::from(replace(&case["source"], &source, &cwd).as_str().unwrap());
        let spidev = if spidev.is_absolute() {
            spidev
        } else {
            cwd.join(spidev)
        };
        fs::create_dir_all(&spidev).unwrap();
        fs::write(spidev.join("spidev_module.c"), b"controlled C extension\n").unwrap();
        fs::write(spidev.join("LICENSE"), b"controlled spidev license\n").unwrap();
        if name == "missing-license" {
            fs::remove_file(spidev.join("LICENSE")).unwrap();
        }
        if name == "missing-module" {
            fs::remove_file(spidev.join("spidev_module.c")).unwrap();
        }
        let out = PathBuf::from(replace(&case["output"], &source, &cwd).as_str().unwrap());
        let out = if out.is_absolute() {
            out
        } else {
            cwd.join(out)
        };
        if name == "output-collision" {
            fs::create_dir_all(&out).unwrap();
            fs::write(out.join("owned-marker"), b"owned marker").unwrap();
        }
        let commands = cwd.join("commands");
        fs::create_dir(&commands).unwrap();
        for program in ["clang++", "cc"] {
            if (name == "first-spawn" && program == "clang++")
                || (name == "second-spawn" && program == "cc")
            {
                continue;
            }
            let p = commands.join(program);
            fs::copy(fixtures().join("compiler.sh"), &p).unwrap();
            fs::set_permissions(p, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let args: Vec<String> =
            serde_json::from_value(replace(&case["args"], &source, &cwd)).unwrap();
        let include = replace(&case["python_include"], &source, &cwd);
        let captured = Command::new(env!("CARGO_BIN_EXE_build-pandad-firmware-spi-fixture"))
            .args([
                "--root",
                source.to_str().unwrap(),
                "--python-include",
                include.as_str().unwrap(),
                "--extension-suffix",
                case["extension_suffix"].as_str().unwrap(),
            ])
            .args(args)
            .current_dir(&cwd)
            .env("PATH", &commands)
            .env("ARGV_DIR", &cwd)
            .env("FIXTURE_CASE", name)
            .output()
            .unwrap();
        assert_eq!(
            captured.status.code(),
            Some(case["exit_code"].as_i64().unwrap() as i32),
            "{name}: {}",
            String::from_utf8_lossy(&captured.stderr)
        );
        if case["help"] == true {
            let help = String::from_utf8(captured.stdout).unwrap();
            for flag in ["--output", "--json11-prefix", "--spidev-source"] {
                assert!(help.contains(flag));
            }
        } else if case["report"].is_null() {
            assert!(captured.stdout.is_empty(), "{name}");
        } else {
            assert_eq!(
                serde_json::from_slice::<Value>(&captured.stdout).unwrap(),
                replace(&case["report"], &source, &cwd),
                "{name}"
            );
        }
        let mut actual = serde_json::Map::new();
        files(&out, &out, &mut actual);
        assert_eq!(
            Value::Object(actual),
            replace(&case["files"], &source, &cwd),
            "{name}"
        );
        for program in ["clang++", "cc"] {
            let argfile = cwd.join(format!("{program}.argv"));
            if let Some(expected) = case["argv"].get(program) {
                let bytes = fs::read(argfile).unwrap();
                let actual = bytes
                    .split(|b| *b == 0)
                    .filter(|v| !v.is_empty())
                    .map(|v| String::from_utf8(v.to_vec()).unwrap())
                    .collect::<Vec<_>>();
                assert_eq!(
                    json!(actual),
                    replace(expected, &source, &cwd),
                    "{name} {program}"
                );
            } else {
                assert!(!argfile.exists(), "{name} {program}");
            }
        }
    }
}
struct Peer;
impl Runner for Peer {
    fn run(&mut self, _spec: &CommandSpec) -> Result<Captured> {
        panic!("disk and CLI admission must precede compiler")
    }
}
#[test]
fn new_disk_admission_and_required_explicit_python_metadata_precede_output_creation() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("output");
    let args = vec![
        "--output".into(),
        output.display().to_string(),
        "--json11-prefix".into(),
        "json".into(),
        "--spidev-source".into(),
        "source".into(),
        "--python-include".into(),
        "python".into(),
        "--extension-suffix".into(),
        ".fixture.so".into(),
    ];
    assert!(pandad_firmware_spi_build::run(temp.path(), &args, &mut Peer, 0).is_err());
    assert!(!output.exists());
    for flag in ["--python-include", "--extension-suffix"] {
        let mut missing = args.clone();
        let at = missing.iter().position(|a| a == flag).unwrap();
        missing.drain(at..at + 2);
        assert!(
            pandad_firmware_spi_build::run(temp.path(), &missing, &mut Peer, u64::MAX).is_err()
        );
        assert!(!output.exists());
    }
}
