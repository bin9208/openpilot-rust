//! An accepted date error report must survive a subsequent process failure.
use openpilot_dev_tools::{
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
    timed_check::{run, Kind, Options},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};
struct Peer {
    completions: Vec<Value>,
    calls: usize,
}
impl Runner for Peer {
    fn run(&mut self, _: &CommandSpec) -> Result<Captured> {
        let row = self
            .completions
            .get(self.calls)
            .ok_or("unexpected additional candidate call")?;
        self.calls += 1;
        Ok(Captured {
            code: row["code"].as_i64().unwrap().try_into().unwrap(),
            stdout: row["stdout"].as_str().unwrap().as_bytes().to_vec(),
            stderr: row["stderr"].as_str().unwrap().as_bytes().to_vec(),
        })
    }
}
fn files(directory: &Path, prefix: &Path, rows: &mut serde_json::Map<String, Value>) {
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files(&path, prefix, rows);
        } else {
            rows.insert(
                path.strip_prefix(prefix)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned(),
                json!(fs::read(path)
                    .unwrap()
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()),
            );
        }
    }
}
#[test]
fn original_published_date_comparison_bytes_precede_later_process_failure() {
    let bytes = include_bytes!("fixtures/timed-tools/checker-prefix-original.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "4ba3a9de56de6eb28595d83bcd0ce31e6792ab85ad140233e0c6f8f28228f6fc"
    );
    let original: Value = serde_json::from_slice(bytes).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("artifacts");
    let options = Options {
        root: Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../..")).into(),
        binary: "/controlled/candidate".into(),
        output: output.clone(),
        reference: env!("CARGO_BIN_EXE_timed-source-reference").into(),
        command: env!("CARGO_BIN_EXE_timed-command-fixture").into(),
        worker: vec![],
    };
    let mut peer = Peer {
        completions: original["completions"].as_array().unwrap().clone(),
        calls: 0,
    };
    assert!(run(Kind::Dates, &options, &mut peer).is_err());
    assert_eq!(peer.calls, 2);
    let mut actual = serde_json::Map::new();
    files(&output, &output, &mut actual);
    assert_eq!(Value::Object(actual), original["files_hex"]);
}
