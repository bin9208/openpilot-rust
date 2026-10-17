use openpilot_dev_tools::{
    beep_clock_check::check,
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
struct Peer {
    mode: String,
    calls: usize,
}
impl Runner for Peer {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        self.calls += 1;
        let corpus = unpack(include_bytes!("fixtures/beep-clock/corpus.json.gz"));
        assert_eq!(
            spec.input.as_deref(),
            Some(corpus["input"].as_str().unwrap().as_bytes())
        );
        let argv = spec.effective_argv();
        assert_eq!(argv.len(), 1);
        assert!(Path::new(&argv[0]).is_absolute());
        let mut values = corpus["expected"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        match self.mode.as_str() {
            "mutation" => values[0] = "1".into(),
            "short" => {
                values.pop();
            }
            "extra" => values.push(values[0].clone()),
            "empty" => values.clear(),
            "malformed" => values[0] = "1__2".into(),
            "bom" => values[0].insert(0, '\u{feff}'),
            "nul" => values[0].insert(0, '\0'),
            "integer-lexical" => values[0] = "\u{2003}+０_０\t".into(),
            "large-integer" => values[0] = "9".repeat(4300),
            _ => {}
        }
        let mut stdout = values.iter().map(|v| format!("{v}\n")).collect::<String>();
        if self.mode == "crlf" {
            stdout = stdout.replace('\n', "\r\n");
        }
        if self.mode == "unicode-lines" {
            stdout = stdout.replace('\n', "\u{2028}");
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
fn original_output_overwrite_and_failure_order_are_preserved() {
    let captures = unpack(include_bytes!("fixtures/beep-clock/failures.json.gz"));
    for row in captures.as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let out = temp.path().join("output");
        fs::create_dir(&out).unwrap();
        for name in ["native.stdout", "inputs.json", "summary.json"] {
            fs::write(out.join(name), b"keep").unwrap();
        }
        let mut peer = Peer {
            mode: row["mode"].as_str().unwrap().into(),
            calls: 0,
        };
        let mut printed = vec![];
        let result = check(
            &root(),
            Path::new("candidate"),
            &out,
            &mut peer,
            &mut printed,
        );
        assert_eq!(
            result.is_ok(),
            row["error"].is_null(),
            "{}: {result:?}",
            row["mode"]
        );
        assert_eq!(peer.calls, row["calls"].as_u64().unwrap() as usize);
        assert_eq!(
            String::from_utf8(printed).unwrap(),
            row["stdout"].as_str().unwrap(),
            "{}",
            row["mode"]
        );
        for (name, expected) in row["files"].as_object().unwrap() {
            assert_eq!(
                fs::read_to_string(out.join(name)).unwrap(),
                expected.as_str().unwrap(),
                "{} {name}",
                row["mode"]
            );
        }
    }
}
#[test]
fn original_mkdir_and_source_retirement_guards_are_preserved() {
    let temp = tempfile::tempdir().unwrap();
    let out = temp.path().join("nested/output");
    let mut printed = vec![];
    assert!(check(
        &root(),
        Path::new("candidate"),
        &out,
        &mut Peer {
            mode: "good".into(),
            calls: 0
        },
        &mut printed
    )
    .is_ok());
    assert!(out.join("summary.json").is_file());
    let blocked = temp.path().join("blocked");
    fs::write(&blocked, b"keep").unwrap();
    let mut peer = Peer {
        mode: "good".into(),
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
    assert_eq!(fs::read(blocked).unwrap(), b"keep");
    let source = temp.path().join("rust/tools/check_beepd_clock.py");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(source, b"changed source").unwrap();
    let mut peer = Peer {
        mode: "good".into(),
        calls: 0,
    };
    assert!(check(
        temp.path(),
        Path::new("candidate"),
        &temp.path().join("drift"),
        &mut peer,
        &mut vec![]
    )
    .is_err());
    assert_eq!(peer.calls, 0);
}
#[test]
fn original_argument_status_and_help_are_side_effect_free() {
    let cases: Value = serde_json::from_str(include_str!("fixtures/beep-clock/cli.json")).unwrap();
    for row in cases.as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_check-beep-clock"))
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

#[test]
fn artifact_write_failure_preserves_later_existing_receipts() {
    let captures = unpack(include_bytes!("fixtures/beep-clock/io-failures.json.gz"));
    for row in captures.as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let out = temp.path().join("output");
        fs::create_dir(&out).unwrap();
        for name in ["native.stdout", "inputs.json", "summary.json"] {
            if name == row["blocked"].as_str().unwrap() {
                fs::create_dir(out.join(name)).unwrap();
            } else {
                fs::write(out.join(name), b"keep").unwrap();
            }
        }
        let mut peer = Peer {
            mode: "good".into(),
            calls: 0,
        };
        let mut printed = vec![];
        assert!(check(
            &root(),
            Path::new("candidate"),
            &out,
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
                assert_eq!(fs::read_to_string(out.join(name)).unwrap(), expected);
            } else {
                assert!(out.join(name).is_dir());
            }
        }
    }
}
