use openpilot_dev_tools::{
    qa_process::{Captured, CommandSpec, Runner},
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
    flate2::read::GzDecoder::new(PACKED)
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
        assert_eq!(spec.timeout, std::time::Duration::from_secs(TIMEOUT));
        assert_eq!(spec.stdout_limit, usize::MAX);
        assert_eq!(spec.stderr_limit, usize::MAX);
        assert_eq!(spec.input.as_deref(), Some(self.corpus.input.as_bytes()));
        assert_eq!(spec.effective_argv().len(), if CALLBACKS { 2 } else { 1 });
        assert!(Path::new(&spec.effective_argv()[0]).is_absolute());
        if CALLBACKS {
            let translations = spec.effective_argv()[1].clone();
            assert!(Path::new(&translations).is_absolute());
            assert!(Path::new(&translations).ends_with("openpilot/selfdrive/ui/translations"));
        }
        if self.mode == "timeout" {
            return Err("controlled source timeout".into());
        }
        if let Some(path) = &self.blocked {
            fs::create_dir(path)?;
        }
        let mut text = self.corpus.source.clone();
        let mut stderr = String::new();
        match self.mode {
            "mutation" => {
                let end = text.find('\n').unwrap();
                text.replace_range(..end, "{}");
            }
            "surrogate" => {
                let end = text.find('\n').unwrap();
                text.replace_range(..end, "{\"unexpected\":\"\\ud800\"}");
            }
            "short" => text.truncate(text[..text.len() - 1].rfind('\n').unwrap() + 1),
            "extra" => text.push_str(&(text.lines().next().unwrap().to_owned() + "\n")),
            "empty" => text.clear(),
            "malformed" => text = "bad-json\n".into(),
            "bom" => text.insert(0, '\u{feff}'),
            "nul" => text.insert(0, '\0'),
            "stderr" => stderr = "source-independent diagnostic\n".into(),
            "crlf" => text = text.replace('\n', "\r\n"),
            "unicode-lines" => text = text.replace('\n', "\u{2028}"),
            "numeric-equality" => {
                text = if text.contains("false") {
                    text.replacen("false", "0", 1)
                } else if text.contains("true") {
                    text.replacen("true", "1", 1)
                } else {
                    text.replacen("\"duration\": 0", "\"duration\": 0.0", 1)
                }
            }
            _ => {}
        }
        Ok(Captured {
            code: if self.mode == "exit" { 7 } else { 0 },
            stdout: if self.mode == "invalid-stdout" {
                vec![255]
            } else {
                text.into_bytes()
            },
            stderr: if self.mode == "invalid-stderr" {
                vec![255]
            } else {
                stderr.into_bytes()
            },
        })
    }
}
fn files(output: &Path) -> Value {
    let mut files = serde_json::Map::new();
    for entry in fs::read_dir(output).unwrap() {
        let entry = entry.unwrap();
        if entry.path().is_file() {
            let bytes = fs::read(entry.path()).unwrap();
            files.insert(
                entry.file_name().to_str().unwrap().into(),
                serde_json::json!({"bytes":bytes.len(),"sha256":sha(&bytes)}),
            );
        }
    }
    Value::Object(files)
}
#[test]
fn full_original_corpus_and_candidate_failure_artifact_order() {
    let corpus = corpus();
    let failures: Value = serde_json::from_str(FAILURES).unwrap();
    for row in failures.as_array().unwrap() {
        let temporary = tempfile::tempdir().unwrap();
        let binary = temporary.path().join("candidate");
        let output = temporary.path().join("output");
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
        assert_eq!(files(&output), row["files"], "{mode}");
    }
}
#[test]
fn all_live_source_absence_drift_retirement_and_existing_output_are_guarded() {
    let corpus = corpus();
    let temporary = tempfile::tempdir().unwrap();
    let output = temporary.path().join("existing");
    fs::create_dir(&output).unwrap();
    fs::write(output.join("sentinel"), b"keep").unwrap();
    let mut peer = Peer {
        mode: "invalid-stdout",
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
    let manifest: Value = serde_json::from_str(MANIFEST).unwrap();
    let pins = manifest["sources"].as_object().unwrap();
    for name in pins.keys() {
        let target = temporary.path().join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        match fs::copy(root().join(name), target) {
            Err(error) if name == CHECKER && error.kind() == std::io::ErrorKind::NotFound => {}
            result => {
                result.unwrap();
            }
        }
    }
    for (index, name) in pins
        .keys()
        .filter(|name| name.as_str() != CHECKER)
        .enumerate()
    {
        let source = temporary.path().join(name);
        let bytes = fs::read(&source).unwrap();
        fs::write(&source, b"changed live source").unwrap();
        let output = temporary.path().join(format!("drift-{index}"));
        assert!(check(
            temporary.path(),
            Path::new("candidate"),
            &output,
            &mut peer,
            &mut vec![]
        )
        .is_err());
        assert!(!output.exists());
        fs::remove_file(&source).unwrap();
        let output = temporary.path().join(format!("missing-{index}"));
        assert!(check(
            temporary.path(),
            Path::new("candidate"),
            &output,
            &mut peer,
            &mut vec![]
        )
        .is_err());
        assert!(!output.exists());
        assert_eq!(peer.calls, 0);
        fs::write(source, bytes).unwrap();
    }
    let checker = temporary.path().join(CHECKER);
    fs::write(&checker, b"changed reintroduced checker").unwrap();
    let output = temporary.path().join("reintroduced");
    assert!(check(
        temporary.path(),
        Path::new("candidate"),
        &output,
        &mut peer,
        &mut vec![]
    )
    .is_err());
    assert_eq!(peer.calls, 0);
    assert!(!output.exists());
    fs::remove_file(checker).unwrap();
    if CALLBACKS {
        let added = temporary
            .path()
            .join("openpilot/selfdrive/ui/translations/added.po");
        fs::write(&added, b"new translation source").unwrap();
        let output = temporary.path().join("added-translation");
        assert!(check(
            temporary.path(),
            Path::new("candidate"),
            &output,
            &mut peer,
            &mut vec![]
        )
        .is_err());
        assert_eq!(peer.calls, 0);
        assert!(!output.exists());
        fs::remove_file(added).unwrap();
    }
    let output = temporary.path().join("retired");
    assert!(check(
        temporary.path(),
        Path::new("candidate"),
        &output,
        &mut peer,
        &mut vec![]
    )
    .is_err());
    assert_eq!(peer.calls, 1, "only retired fixed checker may be absent");
}
#[test]
fn original_cli_help_and_argument_errors_create_no_artifacts() {
    let cases: Value = serde_json::from_str(CLI).unwrap();
    for row in cases.as_array().unwrap() {
        let temporary = tempfile::tempdir().unwrap();
        let result = std::process::Command::new(BIN)
            .args(
                row["args"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|value| value.as_str().unwrap()),
            )
            .current_dir(temporary.path())
            .output()
            .unwrap();
        assert_eq!(result.status.code(), row["exit"].as_i64().map(|v| v as i32));
        assert_eq!(!result.stdout.is_empty(), row["help"] == true);
        assert_eq!(fs::read_dir(temporary.path()).unwrap().count(), 0);
    }
}

#[test]
fn artifact_and_print_failures_preserve_exact_original_partial_output() {
    struct BrokenPrint;
    impl std::io::Write for BrokenPrint {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("controlled stdout failure"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let corpus = corpus();
    let rows: Value = serde_json::from_str(IO_FAILURES).unwrap();
    for row in rows.as_array().unwrap() {
        let temporary = tempfile::tempdir().unwrap();
        let binary = temporary.path().join("candidate");
        let output = temporary.path().join("output");
        fs::write(&binary, b"independent-capture-peer").unwrap();
        let blocked = row["blocked"].as_str().unwrap();
        let mut peer = Peer {
            mode: "good",
            corpus: &corpus,
            calls: 0,
            blocked: (blocked != "printed-stdout").then(|| output.join(blocked)),
        };
        let result = if blocked == "printed-stdout" {
            check(&root(), &binary, &output, &mut peer, &mut BrokenPrint)
        } else {
            check(&root(), &binary, &output, &mut peer, &mut vec![])
        };
        assert!(result.is_err(), "{blocked}");
        assert_eq!(peer.calls, row["calls"].as_u64().unwrap() as usize);
        assert_eq!(files(&output), row["files"], "{blocked}");
    }
}
