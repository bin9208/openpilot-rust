//! Actual PUSH transport and original native() log decoding/artifact order.
use openpilot_dev_tools::{
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
    timed_fixtures::{native, Fixture},
};
use openpilot_log_format::JsonValue;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

fn unhex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
struct Peer<'a> {
    row: &'a Value,
    prefix: &'a str,
    socket: Option<zmq::Socket>,
    context: zmq::Context,
}
impl Runner for Peer<'_> {
    fn run(&mut self, _: &CommandSpec) -> Result<Captured> {
        let socket = self.context.socket(zmq::PUSH)?;
        socket.set_linger(1000)?;
        socket.set_sndtimeo(1000)?;
        socket.connect(&format!("ipc:///tmp/logmessage{}", self.prefix))?;
        let frames = self.row["frames_hex"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| unhex(value.as_str().unwrap()))
            .collect::<Vec<_>>();
        if self.row["multipart"] == true {
            socket.send_multipart(&frames, 0)?;
        } else {
            for frame in &frames {
                socket.send(frame.as_slice(), 0)?;
            }
        }
        self.socket = Some(socket);
        Ok(Captured {
            code: 0,
            stdout: b"{\"result\":{\"ok\":null}}\n".to_vec(),
            stderr: vec![],
        })
    }
}
fn json_values(rows: &[JsonValue]) -> Vec<String> {
    rows.iter().map(|row| row.to_json().unwrap()).collect()
}
fn expected(rows: &Value) -> Vec<String> {
    rows.as_array()
        .unwrap()
        .iter()
        .map(|row| {
            JsonValue::parse(row.as_str().unwrap())
                .unwrap()
                .to_json()
                .unwrap()
        })
        .collect()
}
#[test]
fn original_push_frames_pretty_bytes_and_malformed_log_failure_order() {
    let bytes = include_bytes!("fixtures/timed-tools/native-logs-original.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "500fb43572737961a7c8386159590e6b1fc5ded1cbe7ead07e746c1b8ecd252c"
    );
    let data: Value = serde_json::from_slice(bytes).unwrap();
    assert_eq!(data["cases"].as_array().unwrap().len(), 8);
    for row in data["cases"].as_array().unwrap() {
        let temporary = tempfile::tempdir().unwrap();
        let fixture = Fixture::new(
            &temporary.path().join("fixture"),
            "UTC",
            Path::new(env!("CARGO_BIN_EXE_timed-command-fixture")),
        )
        .unwrap();
        let artifacts = temporary.path().join("artifacts");
        fs::create_dir(&artifacts).unwrap();
        let mut peer = Peer {
            row,
            prefix: &fixture.prefix,
            socket: None,
            context: zmq::Context::new(),
        };
        let result = native(
            &fixture,
            Path::new("/controlled/candidate"),
            &artifacts.join("native.jsonl"),
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
        if let Ok((rows, records)) = result {
            assert_eq!(
                json_values(&rows),
                expected(&row["value"]["rows_json"]),
                "{}",
                row["name"]
            );
            assert_eq!(
                json_values(&records),
                expected(&row["value"]["records_json"]),
                "{}",
                row["name"]
            );
        }
        // The collector and its endpoint must be gone even after decoding fails.
        assert!(!Path::new(&format!("/tmp/logmessage{}", fixture.prefix)).exists());
        drop(peer);
        let mut files = serde_json::Map::new();
        for entry in fs::read_dir(&artifacts).unwrap() {
            let path = entry.unwrap().path();
            files.insert(
                path.file_name().unwrap().to_str().unwrap().into(),
                json!(fs::read(path)
                    .unwrap()
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()),
            );
        }
        assert_eq!(Value::Object(files), row["files_hex"], "{}", row["name"]);
    }
}
