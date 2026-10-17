use openpilot_dev_tools::{
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
    ui_state_scheduler_check::{check, Kind},
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};
fn inflate(path: &Path) -> Vec<u8> {
    let mut out = vec![];
    flate2::read::GzDecoder::new(fs::File::open(path).unwrap())
        .read_to_end(&mut out)
        .unwrap();
    out
}
struct Peer {
    fixtures: PathBuf,
    calls: Vec<Value>,
    index: usize,
    root: PathBuf,
}
impl Runner for Peer {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        let row = &self.calls[self.index];
        self.index += 1;
        assert_eq!(spec.argv, vec!["controlled-native"]);
        assert_eq!(
            format!("{:x}", Sha256::digest(spec.input.as_ref().unwrap())),
            row["input_sha256"].as_str().unwrap()
        );
        assert_eq!(
            spec.env.get("PYTHONPATH").map(String::as_str),
            if row["env_overlay"].get("PYTHONPATH").is_some() {
                Some(self.root.to_str().unwrap())
            } else {
                None
            }
        );
        assert!(spec.cwd.is_none());
        assert_eq!(spec.stdout_limit, usize::MAX);
        assert_eq!(spec.stderr_limit, usize::MAX);
        if row["spawn_error"] == true {
            return Err("controlled absent candidate".into());
        }
        let stderr = row["stderr_hex"]
            .as_str()
            .unwrap()
            .as_bytes()
            .chunks_exact(2)
            .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
            .collect();
        Ok(Captured {
            code: row["status"].as_i64().unwrap() as i32,
            stdout: inflate(
                &self
                    .fixtures
                    .join("replies")
                    .join(format!("{}.gz", row["reply_sha256"].as_str().unwrap())),
            ),
            stderr,
        })
    }
}
#[test]
fn whole_original_candidate_assertions_and_failure_artifact_ordering() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ui-state-scheduler");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let rows: Value =
        serde_json::from_str(include_str!("fixtures/ui-state-scheduler/failures.json")).unwrap();
    for row in rows.as_array().unwrap() {
        let name = row["case"].as_str().unwrap();
        if name.starts_with("source-") {
            continue;
        }
        let kind = row["kind"].as_str().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("out");
        if name.starts_with("stale-") {
            fs::create_dir(&output).unwrap();
            fs::write(
                output.join(if kind == "state" {
                    "results.json"
                } else {
                    "result.json"
                }),
                b"old-result",
            )
            .unwrap();
            fs::write(output.join("untouched"), b"keep").unwrap();
        }
        let calls = row["calls"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["lane"] == "native")
            .cloned()
            .collect();
        let mut peer = Peer {
            fixtures: fixtures.clone(),
            calls,
            index: 0,
            root: root.clone(),
        };
        let result = check(
            &root,
            Path::new("controlled-native"),
            &output,
            if kind == "state" {
                Kind::State
            } else {
                Kind::Scheduler
            },
            &mut peer,
        );
        assert_eq!(
            result.is_ok(),
            row["success"].as_bool().unwrap(),
            "{kind}/{name}: {result:?}"
        );
        assert_eq!(peer.index, peer.calls.len(), "{kind}/{name}");
        let mut actual = serde_json::Map::new();
        for e in fs::read_dir(&output).unwrap() {
            let p = e.unwrap().path();
            let bytes = fs::read(&p).unwrap();
            actual.insert(p.file_name().unwrap().to_str().unwrap().into(),serde_json::json!({"bytes":bytes.len(),"sha256":format!("{:x}",Sha256::digest(&bytes))}));
        }
        assert_eq!(Value::Object(actual), row["files"], "{kind}/{name}");
        if let Ok(printed) = result {
            assert_eq!(printed, row["stdout"].as_str().unwrap());
        }
    }
}
#[test]
fn source_and_schema_changes_fail_closed_before_artifacts() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ui-state-scheduler");
    for kind in [Kind::State, Kind::Scheduler] {
        let temp = tempfile::tempdir().unwrap();
        let pins: Value =
            serde_json::from_str(include_str!("fixtures/ui-state-scheduler/source-pins.json"))
                .unwrap();
        let key = if matches!(kind, Kind::State) {
            "state"
        } else {
            "scheduler"
        };
        for path in pins[key].as_object().unwrap().keys() {
            let target = temp.path().join(path);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::copy(root.join(path), target).unwrap();
        }
        fs::copy(root.join("uv.lock"), temp.path().join("uv.lock")).unwrap();
        fs::write(
            temp.path().join(if matches!(kind, Kind::State) {
                "opendbc_repo/opendbc/car/car.capnp"
            } else {
                "openpilot/common/display_scheduling.py"
            }),
            b"changed source",
        )
        .unwrap();
        let mut peer = Peer {
            fixtures: fixtures.clone(),
            calls: vec![],
            index: 0,
            root: root.clone(),
        };
        let output = temp.path().join("out");
        assert!(check(
            temp.path(),
            Path::new("controlled-native"),
            &output,
            kind,
            &mut peer
        )
        .is_err());
        assert_eq!(peer.index, 0);
        assert!(!output.exists());
    }
}
struct DeepPeer {
    fixtures: PathBuf,
    index: usize,
    depth: usize,
}
impl Runner for DeepPeer {
    fn run(&mut self, _: &CommandSpec) -> Result<Captured> {
        let name = ["big", "mici", "pc"][self.index];
        self.index += 1;
        let rows: Value =
            serde_json::from_str(include_str!("fixtures/ui-state-scheduler/failures.json"))
                .unwrap();
        let call = &rows
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["kind"] == "state" && r["case"] == "success")
            .unwrap()["calls"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["lane"] == "native" && r["name"] == name)
            .unwrap()
            .clone();
        let raw = inflate(
            &self
                .fixtures
                .join("replies")
                .join(format!("{}.gz", call["reply_sha256"].as_str().unwrap())),
        );
        let mut raw = String::from_utf8(raw).unwrap();
        raw.truncate(raw.len() - 2);
        raw.push_str(&format!(
            ", \"unused\": {}0{}}}]",
            "[".repeat(self.depth),
            "]".repeat(self.depth)
        ));
        Ok(Captured {
            code: 0,
            stdout: raw.into_bytes(),
            stderr: vec![],
        })
    }
}
#[test]
fn ignored_state_fields_still_obey_original_json_depth_limit() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ui-state-scheduler");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for depth in [9995, 9996] {
        let temp = tempfile::tempdir().unwrap();
        let mut peer = DeepPeer {
            fixtures: fixtures.clone(),
            index: 0,
            depth,
        };
        let output = temp.path().join("out");
        let result = check(
            &root,
            Path::new("controlled-native"),
            &output,
            Kind::State,
            &mut peer,
        );
        assert_eq!(result.is_ok(), depth == 9995, "{result:?}");
        assert_eq!(peer.index, if depth == 9995 { 3 } else { 1 });
        assert_eq!(output.join("results.json").exists(), depth == 9995);
    }
}
#[test]
fn original_cli_help_errors_and_no_side_effects() {
    let rows: Value =
        serde_json::from_str(include_str!("fixtures/ui-state-scheduler/cli.json")).unwrap();
    for row in rows.as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let binary = if row["kind"] == "state" {
            env!("CARGO_BIN_EXE_check-ui-state")
        } else {
            env!("CARGO_BIN_EXE_check-ui-scheduler")
        };
        let result = std::process::Command::new(binary)
            .args(
                row["args"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap()),
            )
            .current_dir(temp.path())
            .output()
            .unwrap();
        assert_eq!(
            result.status.code(),
            Some(row["exit"].as_i64().unwrap() as i32),
            "{row}"
        );
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
    }
}
#[cfg(unix)]
#[test]
fn native_cli_losslessly_handles_raw_paths_and_pythonpath() {
    // Original State's pycapnp loader rejects a physical raw root (recorded in
    // raw-root-original.json). This tests native losslessness after removing
    // that source interpreter, not original successful raw-root admission.
    use std::os::unix::{
        ffi::OsStringExt,
        fs::{symlink, PermissionsExt},
    };
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ui-state-scheduler");
    let original = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let rows: Value =
        serde_json::from_str(include_str!("fixtures/ui-state-scheduler/failures.json")).unwrap();
    for kind in ["state", "scheduler"] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp
            .path()
            .join(std::ffi::OsString::from_vec(b"root-\xff".to_vec()));
        fs::create_dir(&root).unwrap();
        for name in ["openpilot", "opendbc_repo", "rust", "uv.lock"] {
            symlink(original.join(name), root.join(name)).unwrap();
        }
        let row = rows
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["kind"] == kind && r["case"] == "success")
            .unwrap();
        let replies = temp.path().join("replies");
        fs::create_dir(&replies).unwrap();
        for call in row["calls"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["lane"] == "native")
        {
            let data = inflate(
                &fixtures
                    .join("replies")
                    .join(format!("{}.gz", call["reply_sha256"].as_str().unwrap())),
            );
            fs::write(
                replies.join(format!("{}.json", call["name"].as_str().unwrap())),
                data,
            )
            .unwrap();
        }
        let binary = temp
            .path()
            .join(std::ffi::OsString::from_vec(b"candidate-\xfe".to_vec()));
        fs::write(&binary,b"#!/bin/sh\nprintf %s \"$PYTHONPATH\" > \"$ENV_CAPTURE\"\ninput=$(cat)\ncase \"$input\" in\n*'\"pc\": true'*) name=pc;;\n*'\"mici\": true'*) name=mici;;\n*'\"frames\":'*) name=big;;\n*'\"enabled\": false'*) name=disabled;;\n*) name=enabled;;\nesac\ncat \"$REPLIES/$name.json\"\n").unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
        let output = temp
            .path()
            .join(std::ffi::OsString::from_vec(b"output-\xfd".to_vec()));
        let envfile = temp.path().join("env");
        let command = if kind == "state" {
            env!("CARGO_BIN_EXE_check-ui-state")
        } else {
            env!("CARGO_BIN_EXE_check-ui-scheduler")
        };
        let result = std::process::Command::new(command)
            .arg("--binary")
            .arg(&binary)
            .arg("--output")
            .arg(&output)
            .env("OPENPILOT_RUST_ROOT", &root)
            .env("PYTHONPATH", "original-overlay")
            .env("ENV_CAPTURE", &envfile)
            .env("REPLIES", &replies)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(result.stdout, row["stdout"].as_str().unwrap().as_bytes());
        use std::os::unix::ffi::OsStrExt;
        assert_eq!(
            fs::read(envfile).unwrap(),
            if kind == "state" {
                root.as_os_str().as_bytes()
            } else {
                b"original-overlay"
            }
        );
        let mut actual = serde_json::Map::new();
        for entry in fs::read_dir(&output).unwrap() {
            let p = entry.unwrap().path();
            let bytes = fs::read(&p).unwrap();
            actual.insert(p.file_name().unwrap().to_str().unwrap().into(),serde_json::json!({"bytes":bytes.len(),"sha256":format!("{:x}",Sha256::digest(&bytes))}));
        }
        assert_eq!(Value::Object(actual), row["files"]);
    }
}
