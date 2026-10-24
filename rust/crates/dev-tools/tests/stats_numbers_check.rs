use openpilot_dev_tools::{
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
    stats_numbers_check::check,
};
use serde_json::Value;
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn rows() -> Vec<Value> {
    let mut b = vec![];
    flate2::read::GzDecoder::new(
        include_bytes!("fixtures/stats-numbers/corpus.json.gz").as_slice(),
    )
    .read_to_end(&mut b)
    .unwrap();
    serde_json::from_slice(&b).unwrap()
}
struct Peer(String);
impl Runner for Peer {
    fn run(&mut self, s: &CommandSpec) -> Result<Captured> {
        let rows = rows();
        let input = rows
            .iter()
            .map(|r| format!("{}\n", r["input"].as_str().unwrap()))
            .collect::<String>();
        assert_eq!(s.input.as_deref(), Some(input.as_bytes()));
        let mut values = rows
            .iter()
            .map(|r| r["expected"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        match self.0.as_str() {
            "mutation" => values[0] = "changed".into(),
            "short" => {
                values.pop();
            }
            "extra" => values.push(values[0].clone()),
            _ => {}
        }
        let mut text = values
            .iter()
            .map(|v| format!("{}\n", serde_json::to_string(v).unwrap()))
            .collect::<String>();
        match self.0.as_str() {
            "empty" => text.clear(),
            "malformed" => text = "{bad}\n".into(),
            "bom" => text.insert(0, '\u{feff}'),
            "nul" => text.insert(0, '\0'),
            "crlf" => text = text.replace('\n', "\r\n"),
            "unicode-lines" => text = text.replace('\n', "\u{2028}"),
            _ => {}
        }
        Ok(Captured {
            code: if self.0 == "exit" { 7 } else { 0 },
            stdout: if self.0 == "invalid-stdout" {
                vec![255]
            } else {
                text.into_bytes()
            },
            stderr: if self.0 == "invalid-stderr" {
                vec![255]
            } else {
                vec![]
            },
        })
    }
}
#[test]
fn all_original_cases_and_failure_report_boundaries() {
    assert_eq!(rows().len(), 598);
    let captures: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/stats-numbers/failures.json")).unwrap();
    for row in captures {
        let t = tempfile::tempdir().unwrap();
        let b = t.path().join("candidate");
        fs::write(&b, b"owned fixture candidate").unwrap();
        let out = t.path().join("report.json");
        let mode = row["mode"].as_str().unwrap();
        let mut printed = vec![];
        let result = check(
            &root(),
            &b,
            &out,
            None,
            &mut Peer(mode.into()),
            &mut printed,
        );
        assert_eq!(result.is_ok(), row["error"].is_null(), "{mode}: {result:?}");
        assert_eq!(
            String::from_utf8(printed).unwrap(),
            row["stdout"].as_str().unwrap(),
            "{mode}"
        );
        assert_eq!(
            fs::read_to_string(out).ok().as_deref(),
            row["report"].as_str(),
            "{mode}"
        );
    }
}

#[test]
fn source_drift_fails_before_execution_and_keeps_existing_report() {
    struct Never;
    impl Runner for Never {
        fn run(&mut self, _: &CommandSpec) -> Result<Captured> {
            panic!("must fail before execution");
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("openpilot/system/statsd.py");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::copy(root().join("openpilot/system/statsd.py"), &source).unwrap();
    let checker = temp.path().join("rust/tools/check_stats_numbers.py");
    fs::create_dir_all(checker.parent().unwrap()).unwrap();
    fs::write(&checker, b"changed generator").unwrap();
    let output = temp.path().join("report.json");
    fs::write(&output, b"keep").unwrap();
    assert!(check(
        temp.path(),
        Path::new("missing"),
        &output,
        None,
        &mut Never,
        &mut vec![]
    )
    .is_err());
    fs::remove_file(checker).unwrap();
    fs::write(source, b"changed aggregation").unwrap();
    assert!(check(
        temp.path(),
        Path::new("missing"),
        &output,
        None,
        &mut Never,
        &mut vec![]
    )
    .is_err());
    assert_eq!(fs::read(output).unwrap(), b"keep");
}

#[test]
fn pathlike_runner_normalization_and_report_parent_are_preserved() {
    struct Reject;
    impl Runner for Reject {
        fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
            assert_eq!(spec.effective_argv(), ["runner", "candidate"]);
            Err("reached original-compatible argv".into())
        }
    }
    let result = check(
        &root(),
        Path::new("./candidate"),
        Path::new("unused"),
        Some(Path::new("./runner")),
        &mut Reject,
        &mut vec![],
    );
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("original-compatible argv"));
    let temp = tempfile::tempdir().unwrap();
    let binary = temp.path().join("candidate");
    fs::write(&binary, b"owned fixture candidate").unwrap();
    let output = temp.path().join("missing/report.json");
    let mut printed = vec![];
    assert!(check(
        &root(),
        &binary,
        &output,
        None,
        &mut Peer("good".into()),
        &mut printed
    )
    .is_err());
    assert!(!output.parent().unwrap().exists());
    assert!(printed.is_empty());
    assert_eq!(
        std::process::Command::new(env!("CARGO_BIN_EXE_check-stats-numbers"))
            .arg("--unknown")
            .output()
            .unwrap()
            .status
            .code(),
        Some(2)
    );
}
