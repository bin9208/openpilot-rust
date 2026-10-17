use openpilot_dev_tools::{
    hardware_control_policy_check::check,
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
        include_bytes!("fixtures/hardware-control-policy/corpus.json.gz").as_slice(),
    )
    .read_to_end(&mut bytes)
    .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}
struct Peer {
    mode: String,
}
impl Runner for Peer {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        let rows = rows();
        let expected = rows
            .iter()
            .map(|r| format!("{}\n", r["input"].as_str().unwrap()))
            .collect::<String>();
        assert_eq!(spec.input.as_deref(), Some(expected.as_bytes()));
        let mut values = rows
            .iter()
            .map(|r| r["expected"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        match self.mode.as_str() {
            "mutation" => {
                let mut value: Value = serde_json::from_str(&values[0]).unwrap();
                value["outcomes"][0]["value"] = "changed".into();
                values[0] = value.to_string();
            }
            "short" => {
                values.pop();
            }
            "extra" => values.push(values[0].clone()),
            "utf16-text" => values[0] = values[0].chars().flat_map(|ch| [ch, '\0']).collect(),
            "depth" => {
                values[0] = format!(
                    "{{\"events\":{}0{},{}",
                    "[".repeat(10000),
                    "]".repeat(10000),
                    &values[0][1..]
                )
            }
            _ => {}
        }
        let mut stdout = values.iter().map(|v| format!("{v}\n")).collect::<String>();
        match self.mode.as_str() {
            "empty" => stdout.clear(),
            "malformed" => stdout = "{bad}\n".into(),
            "bom" => stdout.insert(0, '\u{feff}'),
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
                vec![]
            },
        })
    }
}
#[test]
fn original_cases_and_failure_artifact_order_are_preserved() {
    let captures: Value = serde_json::from_str(include_str!(
        "fixtures/hardware-control-policy/failures.json"
    ))
    .unwrap();
    assert_eq!(rows().len(), 35);
    for row in captures.as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("out");
        let mut printed = Vec::new();
        let mode = row["mode"].as_str().unwrap();
        let result = check(
            &root(),
            Path::new("candidate"),
            &output,
            &[],
            &mut Peer { mode: mode.into() },
            &mut printed,
        );
        assert_eq!(result.is_ok(), row["error"].is_null(), "{mode}: {result:?}");
        let mut files = fs::read_dir(&output)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        files.sort();
        assert_eq!(serde_json::to_value(files).unwrap(), row["files"], "{mode}");
        if result.is_ok() {
            assert_eq!(
                String::from_utf8(printed).unwrap(),
                rows()
                    .iter()
                    .map(|r| format!("{} PASS\n", r["name"].as_str().unwrap()))
                    .collect::<String>()
            );
            for expected in rows() {
                let file =
                    fs::read(output.join(format!("{}.json", expected["name"].as_str().unwrap())))
                        .unwrap();
                let observed: Value = serde_json::from_slice(&file).unwrap();
                assert_eq!(
                    observed["source"],
                    serde_json::from_str::<Value>(expected["expected"].as_str().unwrap()).unwrap()
                );
            }
        }
    }
}

#[test]
fn live_policy_drift_and_existing_evidence_fail_before_candidate() {
    struct Never;
    impl Runner for Never {
        fn run(&mut self, _: &CommandSpec) -> Result<Captured> {
            panic!("candidate must not run");
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let manifest: Value = serde_json::from_str(include_str!(
        "fixtures/hardware-control-policy/manifest.json"
    ))
    .unwrap();
    for name in manifest["sources"].as_object().unwrap().keys() {
        if name == "rust/tools/check_hardware_control.py" {
            continue;
        }
        let destination = temp.path().join(name);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::copy(root().join(name), destination).unwrap();
    }
    let output = temp.path().join("out");
    let existing = temp.path().join("existing");
    fs::create_dir(&existing).unwrap();
    fs::write(existing.join("keep"), b"keep").unwrap();
    assert!(check(
        temp.path(),
        Path::new("candidate"),
        &existing,
        &[],
        &mut Never,
        &mut Vec::new()
    )
    .is_err());
    assert_eq!(fs::read(existing.join("keep")).unwrap(), b"keep");
    for name in manifest["sources"].as_object().unwrap().keys() {
        if name == "rust/tools/check_hardware_control.py" {
            continue;
        }
        let destination = temp.path().join(name);
        let original = fs::read(&destination).unwrap();
        fs::write(&destination, b"changed").unwrap();
        assert!(
            check(
                temp.path(),
                Path::new("candidate"),
                &output,
                &[],
                &mut Never,
                &mut Vec::new()
            )
            .is_err(),
            "{name}"
        );
        assert!(!output.exists());
        fs::write(destination, original).unwrap();
    }
}

#[test]
fn reintroduced_changed_generator_and_invalid_arguments_are_rejected() {
    assert_eq!(
        std::process::Command::new(env!("CARGO_BIN_EXE_check-hardware-control-policy"))
            .arg("--unknown")
            .output()
            .unwrap()
            .status
            .code(),
        Some(2)
    );
    struct Never;
    impl Runner for Never {
        fn run(&mut self, _: &CommandSpec) -> Result<Captured> {
            panic!("candidate must not run");
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let manifest: Value = serde_json::from_str(include_str!(
        "fixtures/hardware-control-policy/manifest.json"
    ))
    .unwrap();
    for name in manifest["sources"].as_object().unwrap().keys() {
        let destination = temp.path().join(name);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        if name == "rust/tools/check_hardware_control.py" {
            fs::write(destination, b"changed").unwrap();
        } else {
            fs::copy(root().join(name), destination).unwrap();
        }
    }
    assert!(check(
        temp.path(),
        Path::new("candidate"),
        &temp.path().join("out"),
        &[],
        &mut Never,
        &mut Vec::new()
    )
    .is_err());
}
