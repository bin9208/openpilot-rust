use openpilot_dev_tools::{
    amplifier_policy_check::check,
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
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
    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(
        include_bytes!("fixtures/amplifier-policy/corpus.json.gz").as_slice(),
    )
    .read_to_end(&mut bytes)
    .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}
struct Peer {
    rows: Vec<Value>,
    change: Option<&'static str>,
    calls: usize,
}
impl Runner for Peer {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        self.calls += 1;
        let input = String::from_utf8(spec.input.clone().unwrap()).unwrap();
        let configs: Vec<Value> = input
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect();
        assert_eq!(
            configs,
            self.rows
                .iter()
                .map(|r| serde_json::from_str::<Value>(r["input"].as_str().unwrap()).unwrap())
                .collect::<Vec<_>>()
        );
        assert!(spec
            .effective_argv()
            .last()
            .unwrap()
            .as_encoded_bytes()
            .ends_with(b"candidate"));
        let mut values = self
            .rows
            .iter()
            .map(|r| r["expected"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        // Preserve original f64 decimal lexemes for all untouched rows. Parsing
        // through serde_json without float_roundtrip can round retry backoff.
        let mut first: Value = serde_json::from_str(&values[0]).unwrap();
        match self.change {
            Some("boolean") => first["outcome"]["value"] = 1.into(),
            Some("numeric") => {
                first["registers"][0] = (first["registers"][0].as_u64().unwrap() as f64).into()
            }
            Some("mutation") => first["registers"][0] = 999.into(),
            Some("short") => {
                values.pop();
            }
            Some("extra") => values.push(values[0].clone()),
            _ => {}
        }
        values[0] = first.to_string();
        if self.change == Some("depth") {
            values[0] = format!(
                "{{\"registers\":{}0{},{}",
                "[".repeat(10000),
                "]".repeat(10000),
                &values[0][1..]
            );
        }
        if self.change == Some("utf16-text") {
            values[0] = values[0].chars().flat_map(|ch| [ch, '\0']).collect();
        }
        let mut stdout = values.iter().map(|v| format!("{v}\n")).collect::<String>();
        if self.change == Some("crlf") {
            stdout = stdout.replace('\n', "\r\n");
        }
        if self.change == Some("unicode-lines") {
            stdout = stdout.replace('\n', "\u{2028}");
        }
        if self.change == Some("bom") {
            stdout.insert(0, '\u{feff}');
        }
        Ok(Captured {
            code: if self.change == Some("exit") { 7 } else { 0 },
            stdout: if self.change == Some("empty") {
                vec![]
            } else if self.change == Some("malformed") {
                b"{bad}\n".to_vec()
            } else {
                stdout.into_bytes()
            },
            stderr: if self.change == Some("stderr-utf8") {
                vec![255]
            } else {
                vec![]
            },
        })
    }
}

#[test]
fn discarded_duplicate_cannot_bypass_original_recursion_limit() {
    let temp = tempfile::tempdir().unwrap();
    let binary = temp.path().join("candidate");
    fs::write(&binary, b"fixture").unwrap();
    let output = temp.path().join("evidence");
    let mut peer = Peer {
        rows: rows(),
        change: Some("depth"),
        calls: 0,
    };
    let result = check(&root(), &binary, &output, &[], &mut peer);
    assert!(result.is_err(), "original rejects overwritten deep JSON");
    assert_eq!(fs::read_dir(output).unwrap().count(), 0);
}

#[test]
fn invalid_cli_arguments_use_argparse_exit_status() {
    assert_eq!(
        std::process::Command::new(env!("CARGO_BIN_EXE_check-amplifier-policy"))
            .arg("--unknown")
            .output()
            .unwrap()
            .status
            .code(),
        Some(2)
    );
}

#[test]
fn decoded_utf8_text_must_not_be_reinterpreted_as_utf16() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("evidence");
    let mut peer = Peer {
        rows: rows(),
        change: Some("utf16-text"),
        calls: 0,
    };
    assert!(check(&root(), Path::new("candidate"), &output, &[], &mut peer).is_err());
    assert_eq!(fs::read_dir(output).unwrap().count(), 0);
}

#[test]
fn original_case_policy_drift_fails_before_execution() {
    let temp = tempfile::tempdir().unwrap();
    let source = "openpilot/system/hardware/tici/amplifier.py";
    fs::create_dir_all(temp.path().join(source).parent().unwrap()).unwrap();
    fs::copy(root().join(source), temp.path().join(source)).unwrap();
    let checker = temp.path().join("rust/tools/check_amplifier.py");
    fs::create_dir_all(checker.parent().unwrap()).unwrap();
    fs::write(checker, b"changed original cases").unwrap();
    let mut peer = Peer {
        rows: rows(),
        change: None,
        calls: 0,
    };
    assert!(check(
        temp.path(),
        Path::new("candidate"),
        &temp.path().join("out"),
        &[],
        &mut peer
    )
    .is_err());
    assert_eq!(peer.calls, 0);
}
#[test]
fn preserves_all_original_cases_and_rejects_changed_outputs() {
    let rows = rows();
    assert_eq!(rows.len(), 2034);
    let captures: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/amplifier-policy/failures.json")).unwrap();
    for capture in captures {
        let mode = capture["mode"].as_str().unwrap();
        let change = match mode {
            "numeric" => Some("numeric"),
            "boolean" => Some("boolean"),
            "mutation" => Some("mutation"),
            "short" => Some("short"),
            "extra" => Some("extra"),
            "exit" => Some("exit"),
            "malformed" => Some("malformed"),
            "empty" => Some("empty"),
            "stderr-utf8" => Some("stderr-utf8"),
            "bom" => Some("bom"),
            "crlf" => Some("crlf"),
            "unicode-lines" => Some("unicode-lines"),
            _ => panic!("unknown capture"),
        };
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("evidence");
        let binary = temp.path().join("candidate");
        fs::write(&binary, b"fixture").unwrap();
        let mut peer = Peer {
            rows: rows.clone(),
            change,
            calls: 0,
        };
        let result = check(&root(), &binary, &output, &[], &mut peer);
        assert_eq!(
            result.is_ok(),
            capture["success"] == true,
            "{change:?}: {result:?}"
        );
        assert_eq!(peer.calls, 1);
        assert_eq!(output.join("result.json").exists(), result.is_ok());
        let mut paths = Vec::new();
        for entry in fs::read_dir(&output).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                for file in fs::read_dir(path).unwrap() {
                    paths.push(
                        file.unwrap()
                            .path()
                            .strip_prefix(&output)
                            .unwrap()
                            .to_str()
                            .unwrap()
                            .to_owned(),
                    );
                }
            } else {
                paths.push(
                    path.strip_prefix(&output)
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .to_owned(),
                );
            }
        }
        paths.sort();
        assert_eq!(
            serde_json::to_value(paths).unwrap(),
            capture["files"],
            "{mode}"
        );
        if result.is_ok() {
            let report: Value =
                serde_json::from_slice(&fs::read(output.join("result.json")).unwrap()).unwrap();
            assert_eq!(report["cases"].as_array().unwrap().len(), 2034);
            assert_eq!(
                serde_json::from_slice::<Value>(
                    &fs::read(output.join("mask-0-0/source.json")).unwrap()
                )
                .unwrap(),
                serde_json::from_str::<Value>(rows[0]["expected"].as_str().unwrap()).unwrap()
            );
        }
    }
}
#[test]
fn source_drift_and_existing_evidence_fail_before_candidate() {
    let temp = tempfile::tempdir().unwrap();
    let mut peer = Peer {
        rows: rows(),
        change: None,
        calls: 0,
    };
    assert!(check(
        temp.path(),
        Path::new("candidate"),
        &temp.path().join("missing"),
        &[],
        &mut peer
    )
    .is_err());
    assert_eq!(peer.calls, 0);
    let existing = temp.path().join("existing");
    fs::create_dir(&existing).unwrap();
    fs::write(existing.join("keep"), b"keep").unwrap();
    assert!(check(&root(), Path::new("candidate"), &existing, &[], &mut peer).is_err());
    assert_eq!(peer.calls, 0);
    assert_eq!(fs::read(existing.join("keep")).unwrap(), b"keep");
}
