use openpilot_dev_tools::{
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
    ui_application_logic_check::{check, Kind},
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::Path};
fn inflate(path: &Path) -> Vec<u8> {
    let mut out = vec![];
    flate2::read::GzDecoder::new(fs::File::open(path).unwrap())
        .read_to_end(&mut out)
        .unwrap();
    out
}
fn decode(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
        .collect()
}
struct Peer<'a> {
    fixtures: &'a Path,
    row: &'a Value,
    calls: usize,
}
impl Runner for Peer<'_> {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        self.calls += 1;
        let kind = self.row["kind"].as_str().unwrap();
        assert_eq!(spec.argv, vec!["controlled-native"]);
        assert_eq!(
            spec.input.as_ref(),
            Some(&inflate(
                &self.fixtures.join(format!("{kind}-stdin.json.gz"))
            ))
        );
        assert_eq!(spec.inherit_stderr, kind == "fps");
        assert!(spec.env.is_empty());
        assert!(spec.os_env.is_none());
        assert!(spec.cwd.is_none());
        assert_eq!(spec.stdout_limit, usize::MAX);
        if self.row["case"] == "spawn" {
            return Err("controlled absent candidate".into());
        }
        Ok(Captured {
            code: self.row["status"].as_i64().unwrap() as i32,
            stdout: inflate(
                &self
                    .fixtures
                    .join("replies")
                    .join(format!("{}.gz", self.row["reply_sha256"].as_str().unwrap())),
            ),
            stderr: decode(self.row["stderr_hex"].as_str().unwrap()),
        })
    }
}
#[test]
fn original_subprocess_cases_preserve_assertions_and_artifact_phases() {
    let fixtures =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ui-application-logic");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let rows: Value =
        serde_json::from_str(include_str!("fixtures/ui-application-logic/failures.json")).unwrap();
    for row in rows.as_array().unwrap() {
        let kind = row["kind"].as_str().unwrap();
        let case = row["case"].as_str().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("output");
        if case.starts_with("stale-") {
            fs::create_dir(&output).unwrap();
            for name in [
                "input.json",
                "source.json",
                "native.json",
                "native.stderr",
                "result.json",
            ] {
                fs::write(output.join(name), format!("old-{name}")).unwrap();
            }
            fs::write(output.join("untouched"), b"keep").unwrap();
        }
        let mut peer = Peer {
            fixtures: &fixtures,
            row,
            calls: 0,
        };
        let result = check(
            &root,
            Path::new("controlled-native"),
            &output,
            if kind == "fps" {
                Kind::Fps
            } else {
                Kind::Stack
            },
            &mut peer,
        );
        assert_eq!(
            result.is_ok(),
            row["exit"] == 0,
            "{kind}/{case}: {result:?}"
        );
        assert_eq!(peer.calls, 1);
        let mut actual = serde_json::Map::new();
        for entry in fs::read_dir(&output).unwrap() {
            let p = entry.unwrap().path();
            let bytes = fs::read(&p).unwrap();
            actual.insert(p.file_name().unwrap().to_str().unwrap().into(),serde_json::json!({"bytes":bytes.len(),"sha256":format!("{:x}",Sha256::digest(&bytes))}));
        }
        assert_eq!(Value::Object(actual), row["files"], "{kind}/{case}");
        if let Ok(printed) = result {
            assert_eq!(printed, row["stdout"].as_str().unwrap());
        }
    }
}
#[test]
fn original_cli_admission_has_no_output_side_effects() {
    let rows: Value =
        serde_json::from_str(include_str!("fixtures/ui-application-logic/cli.json")).unwrap();
    for row in rows.as_array().unwrap() {
        let t = tempfile::tempdir().unwrap();
        let binary = if row["kind"] == "fps" {
            env!("CARGO_BIN_EXE_check-ui-fps")
        } else {
            env!("CARGO_BIN_EXE_check-ui-stack")
        };
        let result = std::process::Command::new(binary)
            .args(
                row["args"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap()),
            )
            .current_dir(t.path())
            .output()
            .unwrap();
        assert_eq!(
            result.status.code(),
            Some(row["exit"].as_i64().unwrap() as i32),
            "{row}"
        );
        assert_eq!(fs::read_dir(t.path()).unwrap().count(), 0);
    }
}
struct Deep {
    depth: usize,
}
impl Runner for Deep {
    fn run(&mut self, _: &CommandSpec) -> Result<Captured> {
        Ok(Captured {
            code: 0,
            stdout: format!("{}0{}", "[".repeat(self.depth), "]".repeat(self.depth)).into_bytes(),
            stderr: vec![],
        })
    }
}
#[test]
fn independent_original_decoder_and_fps_serializer_depth_artifacts() {
    let rows: Value =
        serde_json::from_str(include_str!("fixtures/ui-application-logic/depth.json")).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for row in rows.as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("out");
        let kind = if row["kind"] == "fps" {
            Kind::Fps
        } else {
            Kind::Stack
        };
        assert!(check(
            &root,
            Path::new("controlled-native"),
            &output,
            kind,
            &mut Deep {
                depth: row["total_depth"].as_u64().unwrap() as usize
            }
        )
        .is_err());
        let mut files = fs::read_dir(&output)
            .unwrap()
            .map(|p| p.unwrap().file_name().to_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        files.sort();
        assert_eq!(serde_json::json!(files), row["files"], "{row}");
    }
}
#[test]
fn changed_application_source_fails_before_candidate_or_artifacts() {
    struct Never;
    impl Runner for Never {
        fn run(&mut self, _: &CommandSpec) -> Result<Captured> {
            panic!("source mismatch must not invoke candidate")
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("openpilot/system/ui/lib/application.py");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(source, b"changed source").unwrap();
    for kind in [Kind::Fps, Kind::Stack] {
        let out = temp.path().join(format!("out-{kind:?}"));
        assert!(check(temp.path(), Path::new("candidate"), &out, kind, &mut Never).is_err());
        assert!(out.is_dir());
        assert_eq!(fs::read_dir(out).unwrap().count(), 0);
    }
}
#[test]
fn real_native_cli_raw_paths_and_fps_inherited_stderr() {
    use std::os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::PermissionsExt,
    };
    let fixtures =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ui-application-logic");
    let rows: Value =
        serde_json::from_str(include_str!("fixtures/ui-application-logic/failures.json")).unwrap();
    for kind in ["fps", "stack"] {
        let t = tempfile::tempdir().unwrap();
        let root = t
            .path()
            .join(std::ffi::OsString::from_vec(b"root-\xff".to_vec()));
        let source = root.join("openpilot/system/ui/lib/application.py");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(
            source,
            include_bytes!("../../../../openpilot/system/ui/lib/application.py"),
        )
        .unwrap();
        let row = rows
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["kind"] == kind && r["case"] == "success")
            .unwrap();
        let reply = t.path().join("reply");
        fs::write(
            &reply,
            inflate(
                &fixtures
                    .join("replies")
                    .join(format!("{}.gz", row["reply_sha256"].as_str().unwrap())),
            ),
        )
        .unwrap();
        let binary = t
            .path()
            .join(std::ffi::OsString::from_vec(b"peer-\xfe".to_vec()));
        let mut script=b"#!/bin/sh\ncat >/dev/null\nprintf %s \"$RAW_MARKER\" > \"$ENV_CAPTURE\"\ncat \"$REPLY\"\n".to_vec();
        if kind == "fps" {
            script.extend(b"printf '\\377' >&2\n");
        }
        fs::write(&binary, script).unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
        let out = t
            .path()
            .join(std::ffi::OsString::from_vec(b"out-\xfd".to_vec()));
        let envcapture = t.path().join("env");
        let raw = std::ffi::OsString::from_vec(b"value-\xfc".to_vec());
        let command = if kind == "fps" {
            env!("CARGO_BIN_EXE_check-ui-fps")
        } else {
            env!("CARGO_BIN_EXE_check-ui-stack")
        };
        let result = std::process::Command::new(command)
            .arg("--binary")
            .arg(binary)
            .arg("--output")
            .arg(&out)
            .env("OPENPILOT_RUST_ROOT", &root)
            .env("RAW_MARKER", &raw)
            .env("ENV_CAPTURE", &envcapture)
            .env("REPLY", reply)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(result.stdout, row["stdout"].as_str().unwrap().as_bytes());
        assert_eq!(
            result.stderr,
            if kind == "fps" {
                b"\xff".as_slice()
            } else {
                b"".as_slice()
            }
        );
        assert_eq!(fs::read(envcapture).unwrap(), raw.as_bytes());
        let mut files = serde_json::Map::new();
        for e in fs::read_dir(out).unwrap() {
            let p = e.unwrap().path();
            let bytes = fs::read(&p).unwrap();
            files.insert(p.file_name().unwrap().to_str().unwrap().into(),serde_json::json!({"bytes":bytes.len(),"sha256":format!("{:x}",Sha256::digest(bytes))}));
        }
        assert_eq!(Value::Object(files), row["files"]);
    }
}
