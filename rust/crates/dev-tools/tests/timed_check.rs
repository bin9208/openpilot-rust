//! Fixed checker enumeration and unchanged original failure/artifact boundaries.
use openpilot_dev_tools::{
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
    timed_check::{date_cases, reference_cases, run, Kind, Options},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::Path, time::Duration};

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.."))
}
fn corpus(bytes: &[u8], hash: &str) -> Value {
    assert_eq!(format!("{:x}", Sha256::digest(bytes)), hash);
    serde_json::from_slice(bytes).unwrap()
}
#[test]
fn every_original_reference_and_local_date_request_is_generated() {
    let original = corpus(
        include_bytes!("fixtures/timed-tools/reference-original.json"),
        "c9a30ee09a868cfa96a54e6d4a96e3da26dd1c61b1d23390f0e59e075d7e2bf7",
    );
    let expected = original["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let mut row = row.clone();
            for key in ["values", "errors", "state"] {
                row.as_object_mut().unwrap().remove(key);
            }
            row
        })
        .collect::<Vec<_>>();
    let actual = reference_cases();
    assert_eq!(actual.len(), 54);
    assert_eq!(actual, expected);
    let original = corpus(
        include_bytes!("fixtures/timed-tools/dates-original.json"),
        "bea30075403cc174994435c83e07997865053a929997d5bf72506ba062c94774",
    );
    let expected = original["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let mut row = row.clone();
            row.as_object_mut().unwrap().remove("expected");
            row
        })
        .collect::<Vec<_>>();
    let actual = date_cases();
    assert_eq!(actual.len(), 360);
    assert_eq!(actual, expected);
}
struct Peer<'a> {
    row: &'a Value,
    calls: usize,
}
impl Runner for Peer<'_> {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        self.calls += 1;
        assert_eq!(spec.timeout, Duration::from_secs(30));
        assert_eq!(spec.stdout_limit, usize::MAX);
        assert_eq!(spec.stderr_limit, usize::MAX);
        assert_eq!(
            spec.effective_argv(),
            vec![std::ffi::OsString::from("/controlled/candidate")]
        );
        Ok(Captured {
            code: self.row["code"].as_i64().unwrap().try_into().unwrap(),
            stdout: self.row["stdout"].as_str().unwrap().as_bytes().to_vec(),
            stderr: self.row["stderr"].as_str().unwrap().as_bytes().to_vec(),
        })
    }
}
fn files(
    directory: &Path,
    prefix: &Path,
    names: &mut Vec<String>,
    bytes: &mut serde_json::Map<String, Value>,
) {
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files(&path, prefix, names, bytes);
        } else {
            let name = path
                .strip_prefix(prefix)
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned();
            names.push(name.clone());
            if !name.starts_with("source-") {
                bytes.insert(
                    name,
                    json!(fs::read(path)
                        .unwrap()
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>()),
                );
            }
        }
    }
}
#[test]
fn original_checker_process_parse_comparison_and_partial_artifact_order() {
    let original = corpus(
        include_bytes!("fixtures/timed-tools/checker-failures-original.json"),
        "995a6919d289a45d27c47d838d3e179627666a480a51e559aee2733aed9a701d",
    );
    assert_eq!(original["cases"].as_array().unwrap().len(), 19);
    for row in original["cases"].as_array().unwrap() {
        let temporary = tempfile::tempdir().unwrap();
        let output = temporary.path().join("artifacts");
        let options = Options {
            root: root().into(),
            binary: "/controlled/candidate".into(),
            output: output.clone(),
            reference: env!("CARGO_BIN_EXE_timed-source-reference").into(),
            command: env!("CARGO_BIN_EXE_timed-command-fixture").into(),
            worker: row["worker"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect(),
        };
        let kind = match row["kind"].as_str().unwrap() {
            "reference" => Kind::Reference,
            "dates" => Kind::Dates,
            "http" => Kind::Http,
            "logging" => Kind::Logging,
            other => panic!("unexpected original checker {other}"),
        };
        let mut peer = Peer { row, calls: 0 };
        let result = run(kind, &options, &mut peer);
        assert!(result.is_err(), "{}", row["name"]);
        assert_eq!(peer.calls, 1, "{}", row["name"]);
        let mut names = vec![];
        let mut bytes = serde_json::Map::new();
        files(&output, &output, &mut names, &mut bytes);
        names.sort();
        assert_eq!(json!(names), row["files"], "{}", row["name"]);
        assert_eq!(Value::Object(bytes), row["files_hex"], "{}", row["name"]);
    }
}
