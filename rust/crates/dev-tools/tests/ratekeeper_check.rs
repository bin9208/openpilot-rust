use openpilot_dev_tools::{
    qa_process::{Captured, CommandSpec, Runner},
    ratekeeper_check::check,
    seal::Result,
};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};
#[derive(Deserialize)]
struct Corpus {
    input: String,
    source: String,
}
fn corpus() -> Corpus {
    let mut data = vec![];
    flate2::read::GzDecoder::new(include_bytes!("fixtures/ratekeeper/corpus.json.gz").as_slice())
        .read_to_end(&mut data)
        .unwrap();
    serde_json::from_slice(&data).unwrap()
}
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
struct Peer<'a> {
    mode: &'a str,
    corpus: &'a Corpus,
    calls: usize,
    blocked: Option<PathBuf>,
}
impl Runner for Peer<'_> {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        self.calls += 1;
        if let Some(blocked) = &self.blocked {
            fs::create_dir(blocked)?;
        }
        assert_eq!(spec.input.as_deref(), Some(self.corpus.input.as_bytes()));
        assert_eq!(spec.stdout_limit, usize::MAX);
        assert_eq!(spec.stderr_limit, usize::MAX);
        assert_eq!(spec.effective_argv().len(), 1);
        assert!(Path::new(&spec.effective_argv()[0]).is_absolute());
        let mut stdout = self.corpus.source.clone();
        let mut stderr = String::new();
        match self.mode {
            "mutation" => stdout = stdout.replacen("\"frame\": 1,", "\"frame\": 2,", 1),
            "numeric-equality" => stdout = stdout.replacen("\"frame\": 1,", "\"frame\": true,", 1),
            "short" => stdout.truncate(stdout[..stdout.len() - 1].rfind('\n').unwrap() + 1),
            "extra" => stdout.push_str(&(stdout.lines().next().unwrap().to_owned() + "\n")),
            "empty" => stdout.clear(),
            "malformed" => stdout = "bad-json\n".into(),
            "bom" => stdout.insert(0, '\u{feff}'),
            "nul" => stdout.insert(0, '\0'),
            "stderr" => stderr = "source-independent diagnostic\n".into(),
            "crlf" => stdout = stdout.replace('\n', "\r\n"),
            "unicode-lines" => stdout = stdout.replace('\n', "\u{2028}"),
            _ => {}
        }
        Ok(Captured {
            code: if self.mode == "exit" { 7 } else { 0 },
            stdout: if self.mode == "invalid-stdout" {
                vec![255]
            } else {
                stdout.into_bytes()
            },
            stderr: if self.mode == "invalid-stderr" {
                vec![255]
            } else {
                stderr.into_bytes()
            },
        })
    }
}
#[test]
fn all_original_timing_states_output_bytes_and_failure_artifact_order() {
    let corpus = corpus();
    let captures: Value =
        serde_json::from_str(include_str!("fixtures/ratekeeper/failures.json")).unwrap();
    for row in captures.as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let binary = temp.path().join("candidate");
        let output = temp.path().join("output");
        let mode = row["mode"].as_str().unwrap();
        if mode != "missing-binary" {
            fs::write(&binary, b"independent-capture-peer").unwrap();
        }
        let mut peer = Peer {
            mode,
            corpus: &corpus,
            calls: 0,
            blocked: None,
        };
        let mut printed = vec![];
        let result = check(&root(), &binary, &output, &mut peer, &mut printed);
        assert_eq!(result.is_ok(), row["error"].is_null(), "{mode}: {result:?}");
        assert_eq!(peer.calls, row["calls"].as_u64().unwrap() as usize);
        assert_eq!(
            String::from_utf8(printed).unwrap(),
            row["stdout"].as_str().unwrap(),
            "{mode}"
        );
        let mut actual = serde_json::Map::new();
        for entry in fs::read_dir(&output).unwrap() {
            let entry = entry.unwrap();
            let bytes = fs::read(entry.path()).unwrap();
            actual.insert(
                entry.file_name().to_str().unwrap().into(),
                serde_json::json!({"bytes":bytes.len(),"sha256":sha(&bytes)}),
            );
        }
        assert_eq!(Value::Object(actual), row["files"], "{mode}");
    }
}
#[test]
fn existing_output_and_live_source_drift_never_spawn_or_overwrite() {
    let corpus = corpus();
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("output");
    fs::create_dir(&output).unwrap();
    fs::write(output.join("sentinel"), b"keep").unwrap();
    let mut peer = Peer {
        mode: "good",
        corpus: &corpus,
        calls: 0,
        blocked: None,
    };
    assert!(check(
        &root(),
        Path::new("candidate"),
        &output,
        &mut peer,
        &mut vec![]
    )
    .is_err());
    assert_eq!(peer.calls, 0);
    assert_eq!(fs::read(output.join("sentinel")).unwrap(), b"keep");
    let sources: Value =
        serde_json::from_str(include_str!("fixtures/ratekeeper/manifest.json")).unwrap();
    for name in sources["sources"].as_object().unwrap().keys() {
        let target = temp.path().join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        match fs::copy(root().join(name), &target) {
            Err(error)
                if name == "rust/tools/check_selfdrived_ratekeeper.py"
                    && error.kind() == std::io::ErrorKind::NotFound => {}
            result => {
                result.unwrap();
            }
        }
    }
    fs::write(
        temp.path().join("openpilot/common/realtime.py"),
        b"changed original source",
    )
    .unwrap();
    assert!(check(
        temp.path(),
        Path::new("candidate"),
        &temp.path().join("drift"),
        &mut peer,
        &mut vec![]
    )
    .is_err());
    assert_eq!(peer.calls, 0);
    let timing = temp.path().join("openpilot/common/realtime.py");
    fs::copy(root().join("openpilot/common/realtime.py"), &timing).unwrap();
    for name in ["openpilot/common/realtime.py", "openpilot/common/utils.py"] {
        let path = temp.path().join(name);
        let bytes = fs::read(&path).unwrap();
        fs::remove_file(&path).unwrap();
        let output = temp.path().join(format!(
            "missing-{}",
            path.file_name().unwrap().to_str().unwrap()
        ));
        assert!(
            check(
                temp.path(),
                Path::new("candidate"),
                &output,
                &mut peer,
                &mut vec![]
            )
            .is_err(),
            "{name}"
        );
        assert_eq!(peer.calls, 0);
        assert!(
            !output.exists(),
            "live source must fail before output creation"
        );
        fs::write(path, bytes).unwrap();
    }
    let retired = temp
        .path()
        .join("rust/tools/check_selfdrived_ratekeeper.py");
    fs::write(&retired, b"changed reintroduced checker").unwrap();
    let output = temp.path().join("reintroduced-checker");
    assert!(check(
        temp.path(),
        Path::new("candidate"),
        &output,
        &mut peer,
        &mut vec![]
    )
    .is_err());
    assert_eq!(peer.calls, 0);
    assert!(!output.exists());
}
#[test]
fn original_late_artifact_write_failures_preserve_partial_evidence() {
    let corpus = corpus();
    let rows: Value =
        serde_json::from_str(include_str!("fixtures/ratekeeper/io-failures.json")).unwrap();
    for row in rows.as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let binary = temp.path().join("candidate");
        fs::write(&binary, b"independent-capture-peer").unwrap();
        let output = temp.path().join("output");
        let mut peer = Peer {
            mode: "good",
            corpus: &corpus,
            calls: 0,
            blocked: Some(output.join(row["blocked"].as_str().unwrap())),
        };
        let mut printed = vec![];
        assert!(check(&root(), &binary, &output, &mut peer, &mut printed).is_err());
        assert!(printed.is_empty());
        assert_eq!(peer.calls, 1);
        let mut files = serde_json::Map::new();
        for entry in fs::read_dir(&output).unwrap() {
            let entry = entry.unwrap();
            let value = if entry.file_type().unwrap().is_file() {
                let bytes = fs::read(entry.path()).unwrap();
                serde_json::json!({"bytes":bytes.len(),"sha256":sha(&bytes)})
            } else {
                Value::Null
            };
            files.insert(entry.file_name().to_str().unwrap().into(), value);
        }
        assert_eq!(Value::Object(files), row["files"], "{}", row["blocked"]);
    }
}

#[test]
fn original_cli_help_and_argument_errors_remain_side_effect_free() {
    let rows: Value = serde_json::from_str(include_str!("fixtures/ratekeeper/cli.json")).unwrap();
    for row in rows.as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_check-ratekeeper"))
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
        assert_eq!(result.status.code(), row["exit"].as_i64().map(|v| v as i32));
        assert_eq!(!result.stdout.is_empty(), row["help"] == true);
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
    }
}
