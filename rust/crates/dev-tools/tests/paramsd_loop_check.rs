#![cfg(feature = "paramsd-loop-native")]
use openpilot_dev_tools::{
    paramsd_loop_check::{prepare, Options},
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn fixture_options(directory: &Path) -> Options {
    let binary = directory.join("candidate");
    fs::copy(std::env::current_exe().unwrap(), &binary).unwrap();
    let source = directory.join("source");
    for name in [
        "rust/tools/paramsd_loop_fixture.py",
        "rust/tools/paramsd_fixture.py",
        "rust/tools/check_paramsd_loop.py",
        "rust/tools/paramsd_loop_source.py",
        "rust/crates/dev-tools/src/paramsd_loop_fixture_generate.rs",
        "rust/crates/dev-tools/src/paramsd_fixture_generate.rs",
        "rust/crates/dev-tools/src/paramsd_loop_reference.rs",
        "rust/crates/dev-tools/src/paramsd_loop_check.rs",
        "rust/crates/paramsd/examples/paramsd_loop.rs",
    ] {
        let path = source.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::copy(root().join(name), path).unwrap();
    }
    Options {
        root: source,
        binary,
        evidence: directory.join("evidence"),
    }
}

#[derive(Clone, Copy)]
enum Mode {
    Failure,
    Missing,
    Invalid,
    Oversize,
    MutateBinary,
    MutateInput,
    MutateSource,
    Reappear,
}
struct Peer {
    mode: Mode,
    calls: usize,
    binary: PathBuf,
    source: PathBuf,
}
impl Runner for Peer {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        self.calls += 1;
        assert_eq!(spec.effective_argv().len(), 2);
        assert_eq!(spec.input.as_ref().unwrap().first(), Some(&b'{'));
        let raw = PathBuf::from(spec.effective_argv()[1].clone());
        match self.mode {
            Mode::Failure => Ok(Captured {
                code: 13,
                stdout: b"child failed".to_vec(),
                stderr: b"error".to_vec(),
            }),
            Mode::Missing => Ok(Captured {
                code: 0,
                stdout: Vec::new(),
                stderr: Vec::new(),
            }),
            Mode::Invalid => {
                fs::write(raw, b"not JSON")?;
                Ok(Captured {
                    code: 0,
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                })
            }
            Mode::Oversize => {
                fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(raw)?
                    .set_len(32_000_001)?;
                Ok(Captured {
                    code: 0,
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                })
            }
            Mode::MutateBinary => {
                fs::write(&self.binary, b"changed executable")?;
                Ok(Captured {
                    code: 0,
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                })
            }
            Mode::MutateInput => {
                fs::write(raw.parent().unwrap().join("input.json"), b"changed input")?;
                Ok(Captured {
                    code: 0,
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                })
            }
            Mode::MutateSource => {
                fs::write(
                    self.source.join("rust/tools/paramsd_loop_fixture.py"),
                    b"changed source",
                )?;
                Ok(Captured {
                    code: 0,
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                })
            }
            Mode::Reappear => {
                fs::write(
                    self.source.join("rust/tools/check_paramsd_loop.py"),
                    b"unexpected retired checker",
                )?;
                Ok(Captured {
                    code: 0,
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                })
            }
        }
    }
}

#[test]
fn child_failure_missing_or_invalid_raw_and_mutated_inputs_never_publish() {
    for mode in [
        Mode::Failure,
        Mode::Missing,
        Mode::Invalid,
        Mode::Oversize,
        Mode::MutateBinary,
        Mode::MutateInput,
        Mode::MutateSource,
        Mode::Reappear,
    ] {
        let directory = tempfile::tempdir().unwrap();
        let options = fixture_options(directory.path());
        if matches!(mode, Mode::Reappear) {
            fs::remove_file(options.root.join("rust/tools/check_paramsd_loop.py")).unwrap();
        }
        let mut runner = Peer {
            mode,
            calls: 0,
            binary: options.binary.clone(),
            source: options.root.clone(),
        };
        let error = prepare(&options, &mut runner, &AtomicBool::new(false))
            .err()
            .expect("negative case must fail");
        if matches!(mode, Mode::Oversize) {
            assert!(error.to_string().contains("exceeds 32,000,000 bytes"));
        }
        assert_eq!(runner.calls, 1);
        assert!(!options.evidence.join("result.json").exists());
        assert!(options.evidence.join("input.json").is_file());
        assert!(options.evidence.join("process.json").is_file());
        assert!(options.evidence.join("stdout.log").is_file());
        assert!(options.evidence.join("stderr.log").is_file());
    }
}

#[test]
fn cancellation_and_foreign_receipt_do_not_run_or_overwrite() {
    let directory = tempfile::tempdir().unwrap();
    let options = fixture_options(directory.path());
    let mut runner = Peer {
        mode: Mode::Failure,
        calls: 0,
        binary: options.binary.clone(),
        source: options.root.clone(),
    };
    let cancelled = AtomicBool::new(true);
    assert!(prepare(&options, &mut runner, &cancelled).is_err());
    assert_eq!(runner.calls, 0);
    assert!(!options.evidence.join("result.json").exists());

    let directory = tempfile::tempdir().unwrap();
    let options = fixture_options(directory.path());
    fs::create_dir(&options.evidence).unwrap();
    fs::write(options.evidence.join("result.json"), b"foreign receipt").unwrap();
    assert!(prepare(&options, &mut runner, &AtomicBool::new(false)).is_err());
    assert_eq!(runner.calls, 0);
    assert_eq!(
        fs::read(options.evidence.join("result.json")).unwrap(),
        b"foreign receipt"
    );
}

#[test]
fn original_source_alias_is_rejected_before_candidate_invocation() {
    let directory = tempfile::tempdir().unwrap();
    let options = fixture_options(directory.path());
    let source = options.root.join("rust/tools/paramsd_loop_fixture.py");
    fs::remove_file(&source).unwrap();
    std::os::unix::fs::symlink(root().join("rust/tools/paramsd_loop_fixture.py"), &source).unwrap();
    let mut runner = Peer {
        mode: Mode::Failure,
        calls: 0,
        binary: options.binary.clone(),
        source: options.root.clone(),
    };
    assert!(prepare(&options, &mut runner, &AtomicBool::new(false)).is_err());
    assert_eq!(runner.calls, 0);
    assert!(!options.evidence.join("result.json").exists());
}

#[test]
fn both_retired_python_paths_admit_true_absence_but_reject_changed_or_nonregular_entries() {
    for (name, digest) in [
        (
            "rust/tools/paramsd_loop_fixture.py",
            "767f58edf7163edf8eaa493b5453f3bc00f6966772dba932e96a294e3efe3089",
        ),
        (
            "rust/tools/check_paramsd_loop.py",
            "56c9afe7bc9bb21220478f21405ebf5327a931b0403c7cf628203e0b49fd58c3",
        ),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let options = fixture_options(directory.path());
        fs::remove_file(options.root.join(name)).unwrap();
        let mut runner = Peer {
            mode: Mode::Failure,
            calls: 0,
            binary: options.binary.clone(),
            source: options.root.clone(),
        };
        assert!(prepare(&options, &mut runner, &AtomicBool::new(false)).is_err());
        assert_eq!(
            runner.calls, 1,
            "true absence must reach the candidate: {name}"
        );
        let historical: serde_json::Value = serde_json::from_slice(
            &fs::read(options.evidence.join("historical-sources.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(historical[name], digest);
        assert!(!options.evidence.join("result.json").exists());

        for mutation in ["changed", "symlink", "directory"] {
            let directory = tempfile::tempdir().unwrap();
            let options = fixture_options(directory.path());
            let path = options.root.join(name);
            fs::remove_file(&path).unwrap();
            match mutation {
                "changed" => fs::write(&path, b"changed retired source").unwrap(),
                "symlink" => std::os::unix::fs::symlink(root().join(name), &path).unwrap(),
                "directory" => fs::create_dir(&path).unwrap(),
                _ => unreachable!(),
            }
            let mut runner = Peer {
                mode: Mode::Failure,
                calls: 0,
                binary: options.binary.clone(),
                source: options.root.clone(),
            };
            assert!(prepare(&options, &mut runner, &AtomicBool::new(false)).is_err());
            assert_eq!(runner.calls, 0, "{name}: {mutation}");
            assert!(!options.evidence.join("result.json").exists());
        }
    }
}

#[test]
fn script_candidate_cannot_enter_the_native_loop_gate() {
    let directory = tempfile::tempdir().unwrap();
    let options = fixture_options(directory.path());
    fs::write(
        &options.binary,
        b"#!/usr/bin/env python3\nprint('fake pass')\n",
    )
    .unwrap();
    let mut runner = Peer {
        mode: Mode::Failure,
        calls: 0,
        binary: options.binary.clone(),
        source: options.root.clone(),
    };
    let error = prepare(&options, &mut runner, &AtomicBool::new(false))
        .err()
        .expect("script must be rejected");
    assert!(error.to_string().contains("not a native host ELF"));
    assert_eq!(runner.calls, 0);
    assert!(!options.evidence.join("result.json").exists());
}
