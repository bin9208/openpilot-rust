use openpilot_dev_tools::{
    athena_vision_build,
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/athena-vision-build/original.json")
}
const COMPILER: &str = r#"#!/bin/sh
printf '%s\000' "$0" "$@" > "$ARGV_FILE"
printf 'out-1\nout-2\n'
printf 'err-1\n\377\000raw\n' >&2
if [ "${FIXTURE_EXIT:-0}" != 0 ]; then exit "$FIXTURE_EXIT"; fi
while [ "$#" -gt 0 ]; do if [ "$1" = -o ]; then destination=$2; shift; fi; shift; done
printf 'controlled vision peer\n' > "$destination"
"#;
fn replace(s: &str, root: &Path, cwd: &Path, commands: &Path) -> String {
    s.replace("$ROOT", root.to_str().unwrap())
        .replace("$CWD", cwd.to_str().unwrap())
        .replace("$COMMANDS", commands.to_str().unwrap())
}
#[test]
fn original_real_compiler_cli_unicode_path_and_failure_cases() {
    let cases: Vec<Value> = serde_json::from_slice(&fs::read(fixture()).unwrap()).unwrap();
    for case in cases {
        if case["case"] == "low-space" {
            continue;
        }
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        let cwd = temp.path().join("cwd");
        let commands = temp.path().join("commands");
        for p in [&root, &cwd, &commands] {
            fs::create_dir(p).unwrap();
        }
        for name in ["c++", "custom compiler"] {
            let p = commands.join(name);
            fs::write(&p, COMPILER).unwrap();
            fs::set_permissions(p, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let name = case["case"].as_str().unwrap();
        if name == "symlink-parent" {
            fs::create_dir_all(cwd.join("real/subdir")).unwrap();
            symlink(cwd.join("real/subdir"), cwd.join("link")).unwrap();
        }
        if ["overwrite", "compiler-failure-existing"].contains(&name) {
            fs::create_dir(cwd.join("nested")).unwrap();
            fs::write(cwd.join("nested/peer"), b"owned prior binary").unwrap();
        }
        if name == "parent-file" {
            fs::write(cwd.join("nested"), b"owned parent file").unwrap();
        }
        let args: Vec<String> = case["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| replace(v.as_str().unwrap(), &root, &cwd, &commands))
            .collect();
        let argv_file = cwd.join("argv");
        let mut command = Command::new(env!("CARGO_BIN_EXE_build-athena-vision-peer"));
        command
            .args(["--root", root.to_str().unwrap()])
            .args(&args)
            .current_dir(&cwd)
            .env("PATH", &commands)
            .env_remove("CXX")
            .env("CXXFLAGS", "-DTHIS_MUST_BE_IGNORED=1")
            .env("ARGV_FILE", &argv_file)
            .env(
                "FIXTURE_EXIT",
                if ["compiler-failure", "compiler-failure-existing"].contains(&name) {
                    "9"
                } else {
                    "0"
                },
            );
        if let Some(compiler) = case["compiler"].as_str() {
            command.env("CXX", replace(compiler, &root, &cwd, &commands));
        }
        let captured = command.output().unwrap();
        assert_eq!(
            captured.status.code(),
            Some(case["exit_code"].as_i64().unwrap() as i32),
            "{name}: {}",
            String::from_utf8_lossy(&captured.stderr)
        );
        if case["help"] == true {
            assert!(String::from_utf8(captured.stdout)
                .unwrap()
                .to_lowercase()
                .contains("output"));
            continue;
        }
        assert_eq!(
            String::from_utf8(captured.stdout).unwrap(),
            replace(case["stdout"].as_str().unwrap(), &root, &cwd, &commands),
            "{name}"
        );
        let prefix: Vec<u8> = serde_json::from_value(case["stderr_prefix"].clone()).unwrap();
        assert!(captured.stderr.starts_with(&prefix), "{name}");
        if let Some(expected) = case["argv"].as_array() {
            let actual = fs::read(&argv_file).unwrap();
            let actual = actual
                .split(|b| *b == 0)
                .filter(|v| !v.is_empty())
                .map(|v| String::from_utf8(v.to_vec()).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(
                actual[0],
                replace(
                    case["executed_program"].as_str().unwrap(),
                    &root,
                    &cwd,
                    &commands
                ),
                "{name}"
            );
            let actual = actual[1..].to_vec();
            let expected = expected[1..]
                .iter()
                .map(|v| replace(v.as_str().unwrap(), &root, &cwd, &commands))
                .collect::<Vec<_>>();
            assert_eq!(actual, expected, "{name}");
            let out = PathBuf::from(actual.last().unwrap());
            let out = if out.is_absolute() {
                out
            } else {
                cwd.join(out)
            };
            assert_eq!(
                out.parent().unwrap().exists(),
                case["parent_exists"].as_bool().unwrap()
            );
            let hash = if out.is_file() {
                Some(format!("{:x}", Sha256::digest(fs::read(&out).unwrap())))
            } else {
                None
            };
            assert_eq!(
                hash,
                case["binary_sha256"].as_str().map(str::to_owned),
                "{name}"
            );
        } else {
            assert!(!argv_file.exists(), "{name}");
            let out = PathBuf::from(replace(
                case["output"].as_str().unwrap(),
                &root,
                &cwd,
                &commands,
            ));
            let out = if out.is_absolute() {
                out
            } else {
                cwd.join(out)
            };
            assert_eq!(
                out.parent().unwrap().exists(),
                case["parent_exists"].as_bool().unwrap(),
                "{name}"
            );
            assert_eq!(
                out.parent().unwrap().is_file(),
                case["parent_is_file"].as_bool().unwrap(),
                "{name}"
            );
        }
    }
}
struct Peer {
    calls: Vec<CommandSpec>,
}
impl Runner for Peer {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        self.calls.push(spec.clone());
        fs::write(spec.argv.last().unwrap(), b"controlled vision peer\n")?;
        Ok(Captured {
            code: 0,
            stdout: vec![],
            stderr: vec![],
        })
    }
}
#[test]
fn exact_root_disk_floor_precedes_parent_creation_and_preserves_unbounded_inherited_compiler() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    fs::create_dir(&root).unwrap();
    let output = temp.path().join("nested/peer");
    let args = vec![output.display().to_string()];
    let mut peer = Peer { calls: vec![] };
    assert!(athena_vision_build::run(
        &root,
        &args,
        "custom compiler -Dliteral",
        &mut peer,
        26 * 1024_u64.pow(3) - 1
    )
    .is_err());
    assert!(!output.parent().unwrap().exists());
    assert!(peer.calls.is_empty());
    athena_vision_build::run(
        &root,
        &args,
        "custom compiler -Dliteral",
        &mut peer,
        26 * 1024_u64.pow(3),
    )
    .unwrap();
    assert_eq!(peer.calls[0].argv[0], "custom compiler -Dliteral");
    assert_eq!(peer.calls[0].timeout, Duration::MAX);
    assert!(peer.calls[0].inherit_output);
}
