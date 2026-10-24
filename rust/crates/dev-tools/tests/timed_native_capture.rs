//! Original native() subprocess/JSON/artifact phases, before replacing callers.
use openpilot_dev_tools::{
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
    timed_fixtures::{native, Fixture},
};
use openpilot_log_format::JsonValue;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::Path, time::Duration};
struct Peer<'a> {
    row: &'a Value,
    config: &'a Value,
    calls: usize,
}
impl Runner for Peer<'_> {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        self.calls += 1;
        assert_eq!(
            spec.effective_argv(),
            vec![std::ffi::OsString::from("/controlled/candidate")]
        );
        assert_eq!(spec.timeout, Duration::from_secs(30));
        assert_eq!(spec.stdout_limit, usize::MAX);
        assert_eq!(spec.stderr_limit, usize::MAX);
        assert!(spec.input.as_ref().unwrap().ends_with(b"\n"));
        assert_eq!(
            serde_json::from_slice::<Value>(spec.input.as_ref().unwrap()).unwrap(),
            *self.config
        );
        if self.row["name"] == "timeout" {
            return Err("controlled original subprocess timeout".into());
        }
        Ok(Captured {
            code: i32::try_from(self.row["code"].as_i64().unwrap()).unwrap(),
            stdout: if self.row["name"] == "invalid-stdout" {
                vec![255]
            } else {
                self.row["stdout"].as_str().unwrap().as_bytes().to_vec()
            },
            stderr: if self.row["name"] == "invalid-stderr" {
                vec![255]
            } else {
                self.row["stderr"].as_str().unwrap().as_bytes().to_vec()
            },
        })
    }
}
#[test]
fn original_native_process_json_and_artifact_failure_order() {
    let bytes = include_bytes!("fixtures/timed-tools/native-original.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "79d2be2a951318fa22c1f3a8de416160c053d8f19440fc2de861da2d4ff6afaf"
    );
    let data: Value = serde_json::from_slice(bytes).unwrap();
    assert_eq!(data["cases"].as_array().unwrap().len(), 16);
    for row in data["cases"].as_array().unwrap() {
        let temporary = tempfile::tempdir().unwrap();
        let fixture = Fixture::new(
            &temporary.path().join("fixture"),
            "UTC",
            Path::new(env!("CARGO_BIN_EXE_timed-command-fixture")),
        )
        .unwrap();
        let directory = temporary.path().join("artifacts");
        fs::create_dir(&directory).unwrap();
        let output = directory.join("native.jsonl");
        for name in row["directories"].as_array().unwrap() {
            fs::create_dir(directory.join(name.as_str().unwrap())).unwrap();
        }
        let mut peer = Peer {
            row,
            config: &fixture.config,
            calls: 0,
        };
        let result = native(
            &fixture,
            Path::new("/controlled/candidate"),
            &output,
            &mut peer,
        );
        assert_eq!(
            result.is_err(),
            !row["error"].is_null(),
            "{}: {}",
            row["name"],
            result
                .as_ref()
                .err()
                .map(ToString::to_string)
                .unwrap_or_default()
        );
        assert_eq!(peer.calls, 1);
        if let Ok((rows, records)) = result {
            let actual = rows
                .iter()
                .map(JsonValue::to_json)
                .collect::<std::result::Result<Vec<_>, _>>()
                .unwrap();
            let expected = row["value"]["rows_json"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| {
                    JsonValue::parse(value.as_str().unwrap())
                        .unwrap()
                        .to_json()
                        .unwrap()
                })
                .collect::<Vec<_>>();
            assert_eq!(actual, expected, "{}", row["name"]);
            assert!(records.is_empty(), "{}", row["name"]);
        }
        let mut files = serde_json::Map::new();
        for entry in fs::read_dir(&directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_file() {
                files.insert(
                    path.file_name().unwrap().to_str().unwrap().into(),
                    json!(fs::read(path)
                        .unwrap()
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>()),
                );
            }
        }
        assert_eq!(Value::Object(files), row["files_hex"], "{}", row["name"]);
    }
}
