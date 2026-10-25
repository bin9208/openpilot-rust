use openpilot_dev_tools::{
    beep_sleep_check::check,
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
fn unpack(bytes: &[u8]) -> Value {
    let mut plain = vec![];
    flate2::read::GzDecoder::new(bytes)
        .read_to_end(&mut plain)
        .unwrap();
    serde_json::from_slice(&plain).unwrap()
}
fn decode(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .chunks_exact(2)
        .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
        .collect()
}
struct Peer {
    row: Value,
    calls: usize,
}
impl Runner for Peer {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        self.calls += 1;
        let corpus = unpack(include_bytes!("fixtures/beep-sleep/corpus.json.gz"));
        assert_eq!(
            spec.input.as_deref(),
            Some(corpus["input"].as_str().unwrap().as_bytes())
        );
        let argv = spec.effective_argv();
        assert_eq!(argv.len(), 1);
        assert!(Path::new(&argv[0]).is_absolute());
        Ok(Captured {
            code: self.row["code"].as_i64().unwrap() as i32,
            stdout: decode(self.row["response_hex"].as_str().unwrap()),
            stderr: decode(self.row["stderr_hex"].as_str().unwrap()),
        })
    }
}
#[test]
fn original_json_membership_equality_cardinality_and_failure_artifact_order() {
    for row in unpack(include_bytes!("fixtures/beep-sleep/failures.json.gz"))
        .as_array()
        .unwrap()
    {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("output");
        fs::create_dir(&output).unwrap();
        for name in ["native.stdout", "summary.json", "inputs.json"] {
            fs::write(output.join(name), b"keep").unwrap();
        }
        let mut peer = Peer {
            row: row.clone(),
            calls: 0,
        };
        let mut printed = vec![];
        let result = check(
            &root(),
            Path::new("candidate"),
            &output,
            &mut peer,
            &mut printed,
        );
        assert_eq!(
            result.is_ok(),
            row["error"].is_null(),
            "{}: {result:?}",
            row["mode"]
        );
        assert_eq!(
            peer.calls,
            row["calls"].as_u64().unwrap() as usize,
            "{}",
            row["mode"]
        );
        assert_eq!(
            String::from_utf8(printed).unwrap(),
            row["stdout"].as_str().unwrap(),
            "{}",
            row["mode"]
        );
        for (name, expected) in row["files"].as_object().unwrap() {
            assert_eq!(
                fs::read_to_string(output.join(name)).unwrap(),
                expected.as_str().unwrap(),
                "{} {name}",
                row["mode"]
            );
        }
    }
}
#[test]
fn original_decoder_limit_fails_before_native_stdout_but_serializer_limit_fails_after_it() {
    let rows = unpack(include_bytes!("fixtures/beep-sleep/failures.json.gz"));
    for mode in ["depth-9996", "depth-9997"] {
        let row = rows
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["mode"] == mode)
            .unwrap();
        assert!(!row["error"].is_null());
        assert_eq!(row["files"]["summary.json"], "keep");
        assert_eq!(
            row["files"]["native.stdout"] == "keep",
            mode == "depth-9997"
        );
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("output");
        fs::create_dir(&output).unwrap();
        fs::write(output.join("native.stdout"), b"keep").unwrap();
        fs::write(output.join("summary.json"), b"keep").unwrap();
        let mut peer = Peer {
            row: row.clone(),
            calls: 0,
        };
        let mut printed = vec![];
        assert!(check(
            &root(),
            Path::new("candidate"),
            &output,
            &mut peer,
            &mut printed
        )
        .is_err());
        assert!(printed.is_empty());
        assert_eq!(peer.calls, 1);
        for name in ["native.stdout", "summary.json"] {
            assert_eq!(
                fs::read_to_string(output.join(name)).unwrap(),
                row["files"][name].as_str().unwrap(),
                "{mode}: {name}"
            );
        }
    }
}

#[test]
fn artifact_write_failure_preserves_later_existing_receipts() {
    for row in unpack(include_bytes!("fixtures/beep-sleep/io-failures.json.gz"))
        .as_array()
        .unwrap()
    {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("output");
        fs::create_dir(&output).unwrap();
        for (name, expected) in row["files"].as_object().unwrap() {
            if expected.is_null() {
                fs::create_dir(output.join(name)).unwrap();
            } else {
                fs::write(output.join(name), b"keep").unwrap();
            }
        }
        let mut peer = Peer {
            row: row.clone(),
            calls: 0,
        };
        let mut printed = vec![];
        assert!(check(
            &root(),
            Path::new("candidate"),
            &output,
            &mut peer,
            &mut printed
        )
        .is_err());
        assert_eq!(peer.calls, 1);
        assert_eq!(
            String::from_utf8(printed).unwrap(),
            row["stdout"].as_str().unwrap()
        );
        for (name, expected) in row["files"].as_object().unwrap() {
            if let Some(expected) = expected.as_str() {
                assert_eq!(fs::read_to_string(output.join(name)).unwrap(), expected);
            } else {
                assert!(output.join(name).is_dir());
            }
        }
    }
}
#[test]
fn original_mkdir_and_retirement_source_guards() {
    let row = unpack(include_bytes!("fixtures/beep-sleep/failures.json.gz"))[0].clone();
    let temporary = tempfile::tempdir().unwrap();
    let output = temporary.path().join("nested/output");
    assert!(check(
        &root(),
        Path::new("candidate"),
        &output,
        &mut Peer {
            row: row.clone(),
            calls: 0
        },
        &mut vec![]
    )
    .is_ok());
    assert!(output.join("summary.json").is_file());
    assert!(!output.join("inputs.json").exists());
    let blocked = temporary.path().join("blocked");
    fs::write(&blocked, b"keep").unwrap();
    let mut peer = Peer {
        row: row.clone(),
        calls: 0,
    };
    assert!(check(
        &root(),
        Path::new("candidate"),
        &blocked,
        &mut peer,
        &mut vec![]
    )
    .is_err());
    assert_eq!(peer.calls, 0);
    let source = temporary.path().join("rust/tools/check_beepd_sleep.py");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(&source, b"changed").unwrap();
    assert!(check(
        temporary.path(),
        Path::new("candidate"),
        &temporary.path().join("drift"),
        &mut peer,
        &mut vec![]
    )
    .is_err());
    assert_eq!(peer.calls, 0);
    fs::remove_file(source).unwrap();
    assert!(check(
        temporary.path(),
        Path::new("candidate"),
        &temporary.path().join("retired"),
        &mut peer,
        &mut vec![]
    )
    .is_ok());
}
#[test]
fn original_cli_help_and_argument_errors_are_side_effect_free() {
    let rows: Value = serde_json::from_str(include_str!("fixtures/beep-sleep/cli.json")).unwrap();
    for row in rows.as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_check-beep-sleep"))
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
        assert_eq!(output.status.code(), row["exit"].as_i64().map(|v| v as i32));
        assert_eq!(!output.stdout.is_empty(), row["help"] == true);
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
    }
}
