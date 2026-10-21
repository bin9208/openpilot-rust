#![cfg(feature = "desire-reference-native")]
use openpilot_dev_tools::{
    desire_reference_check::{compare_files, compare_values, prepare, Options},
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use serde_json::{json, Value};
use std::{fs, path::Path, sync::atomic::AtomicBool};

fn write_rows(path: &Path, rows: &[Value]) {
    let mut bytes = Vec::new();
    for row in rows {
        bytes.extend(serde_json::to_vec(row).unwrap());
        bytes.push(b'\n');
    }
    fs::write(path, bytes).unwrap();
}
fn corpus(
    root: &Path,
) -> (
    std::path::PathBuf,
    std::path::PathBuf,
    std::path::PathBuf,
    Value,
) {
    let requests = root.join("request.jsonl");
    let references = root.join("reference.jsonl");
    let actual = root.join("actual.jsonl");
    write_rows(
        &requests,
        &[
            json!({"reset":true,"input":{},"config":{},"remote":null,"scenario":"one"}),
            json!({"reset":false,"input":{},"config":{},"remote":null,"scenario":"one"}),
        ],
    );
    write_rows(
        &references,
        &[
            json!({"state":{"lane_change_state":"Off","desire":0,"numeric":0.125},"remote_allowed":false,"params_refreshed":true}),
            json!({"state":{"lane_change_state":"Starting","desire":2,"numeric":0.75},"remote_allowed":true,"params_refreshed":false}),
        ],
    );
    fs::copy(&references, &actual).unwrap();
    let sealed = json!({"scenarios":{"one":2},"state_coverage":{
        "state:Off":1,"state:Starting":1,"desire:0":1,"desire:2":1}});
    (requests, references, actual, sealed)
}

#[test]
fn exact_schema_numeric_tolerance_and_discrete_values() {
    let expected = json!({"state":{"turn":2,"errors":[0.25,0.5],"active":true}});
    let candidate = json!({"state":{"turn":2,"errors":[0.25000000005,0.5],"active":true}});
    assert!(compare_values(&expected, &candidate, "0").unwrap() > 0.0);
    for bad in [
        json!({"state":{"turn":3,"errors":[0.25,0.5],"active":true}}),
        json!({"state":{"turn":2,"errors":[0.25,0.5,0.5],"active":true}}),
        json!({"state":{"turn":2,"errors":[0.25,0.5],"active":false}}),
        json!({"state":{"turn":2,"errors":[0.2500000002,0.5],"active":true}}),
        json!({"state":{"turn":2,"errors":[0.25,0.5],"active":true,"extra":0}}),
    ] {
        assert!(compare_values(&expected, &bad, "0").is_err());
    }
}

#[test]
fn full_line_roster_requires_every_original_and_candidate_row() {
    for change in [
        "none",
        "missing",
        "extra",
        "mismatch",
        "bad_reset",
        "bad_schema",
        "bad_seal",
    ] {
        let root = tempfile::tempdir().unwrap();
        let (requests, references, actual, mut sealed) = corpus(root.path());
        match change {
            "missing" => {
                write_rows(
                    &actual,
                    &[
                        json!({"state":{"lane_change_state":"Off","desire":0,"numeric":0.125},
                    "remote_allowed":false,"params_refreshed":true}),
                    ],
                );
            }
            "extra" => {
                let mut bytes = fs::read(&actual).unwrap();
                bytes.extend_from_slice(b"{}\n");
                fs::write(&actual, bytes).unwrap();
            }
            "mismatch" => {
                let bytes = fs::read_to_string(&actual)
                    .unwrap()
                    .replace("\"desire\":2", "\"desire\":3");
                assert_ne!(bytes, fs::read_to_string(&actual).unwrap());
                fs::write(&actual, bytes).unwrap();
            }
            "bad_reset" => {
                let bytes = fs::read_to_string(&requests)
                    .unwrap()
                    .replace("\"reset\":false", "\"reset\":true");
                fs::write(&requests, bytes).unwrap();
            }
            "bad_schema" => {
                let bytes = fs::read_to_string(&references)
                    .unwrap()
                    .replace("\"params_refreshed\":false,", "");
                assert_ne!(bytes, fs::read_to_string(&references).unwrap());
                fs::write(&references, bytes).unwrap();
            }
            "bad_seal" => sealed["state_coverage"]["desire:2"] = json!(2),
            _ => {}
        }
        let result = compare_files(&requests, &references, &actual, &sealed, 2);
        assert_eq!(result.is_ok(), change == "none", "{change}: {result:?}");
        if let Ok(report) = result {
            assert_eq!(serde_json::to_value(report).unwrap()["frames"], 2);
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Mode {
    Failure,
    Missing,
    MutateSource,
    Reappear,
}
struct Peer {
    mode: Mode,
    root: std::path::PathBuf,
    calls: usize,
}
impl Runner for Peer {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        self.calls += 1;
        assert_eq!(spec.effective_argv().len(), 3);
        match self.mode {
            Mode::MutateSource => fs::write(
                self.root.join("openpilot/common/realtime.py"),
                b"changed source",
            )?,
            Mode::Reappear => fs::write(
                self.root.join("rust/tools/check_desire_reference.py"),
                b"reappeared",
            )?,
            _ => {}
        }
        Ok(Captured {
            code: if matches!(self.mode, Mode::Failure) {
                17
            } else {
                0
            },
            stdout: b"native diagnostic\n".to_vec(),
            stderr: b"native stderr\n".to_vec(),
        })
    }
}
fn private_options(directory: &Path, historical_absent: bool) -> Options {
    let manifest: Value = serde_json::from_slice(include_bytes!(
        "fixtures/desire-original-6e34f78a/manifest.json"
    ))
    .unwrap();
    let root = directory.join("source");
    let actual_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for (name, _) in manifest["source"].as_object().unwrap() {
        if historical_absent && name == "rust/tools/check_desire_reference.py" {
            continue;
        }
        let path = root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::copy(actual_root.join(name), path).unwrap();
    }
    // The historical checker may be absent while its source directory remains.
    // A late reappearance must be a real file mutation, not a failed test write.
    fs::create_dir_all(root.join("rust/tools")).unwrap();
    let binary = directory.join("candidate");
    fs::copy(std::env::current_exe().unwrap(), &binary).unwrap();
    Options {
        root,
        binary,
        output: directory.join("evidence"),
    }
}
#[test]
fn candidate_failure_missing_output_and_late_source_reappearance_leave_no_report() {
    for mode in [
        Mode::Failure,
        Mode::Missing,
        Mode::MutateSource,
        Mode::Reappear,
    ] {
        let directory = tempfile::tempdir().unwrap();
        let options = private_options(directory.path(), matches!(mode, Mode::Reappear));
        let mut peer = Peer {
            mode,
            root: options.root.clone(),
            calls: 0,
        };
        assert!(prepare(&options, &mut peer, &AtomicBool::new(false)).is_err());
        assert_eq!(peer.calls, 1);
        assert!(!options.output.join("report.json").exists());
        assert!(options.output.join("request.jsonl").is_file());
        assert!(options.output.join("reference.jsonl").is_file());
        assert_eq!(
            fs::read(options.output.join("stdout.log")).unwrap(),
            b"native diagnostic\n",
            "{mode:?}"
        );
        assert_eq!(
            fs::read(options.output.join("stderr.log")).unwrap(),
            b"native stderr\n",
            "{mode:?}"
        );
    }
}

#[test]
fn script_candidate_is_rejected_before_child_execution() {
    let directory = tempfile::tempdir().unwrap();
    let options = private_options(directory.path(), false);
    fs::write(&options.binary, b"#!/bin/sh\nexit 0\n").unwrap();
    let mut peer = Peer {
        mode: Mode::Failure,
        root: options.root.clone(),
        calls: 0,
    };
    assert!(prepare(&options, &mut peer, &AtomicBool::new(false)).is_err());
    assert_eq!(peer.calls, 0);
    assert!(!options.output.join("report.json").exists());
}
