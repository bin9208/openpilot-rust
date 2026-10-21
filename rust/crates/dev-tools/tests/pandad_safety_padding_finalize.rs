//! Finalization regression. This file also compiles against the old edb42bf0 API.
#![cfg(all(target_os = "linux", feature = "pandad-safety-padding-native"))]
use openpilot_dev_tools::{
    pandad_safety_padding_check::{self as padding, Options},
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Duration,
};

const SOURCES: [&str; 6] = [
    "rust/tools/check_pandad_safety.py",
    "rust/tools/pandad_safety_source.cc",
    "openpilot/selfdrive/pandad/panda_safety.cc",
    "openpilot/selfdrive/pandad/pandad.h",
    "openpilot/common/params.cc",
    "openpilot/common/params.h",
];
const HISTORICAL: &str = "rust/tools/check_pandad_safety_padding.py";
const MANIFEST_SHA: &str = "3a12023217195181b9f1d05d06803a4b0add0438baf700abfeb5a7ad0f3f22ea";
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pandad-safety-padding/edb42bf0-original")
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn sealed(name: &str) -> Vec<u8> {
    let folder = fixture();
    let manifest = fs::read(folder.join("SHA256SUMS")).unwrap();
    assert_eq!(
        sha(&manifest),
        MANIFEST_SHA,
        "historical capture manifest changed"
    );
    let digest = std::str::from_utf8(&manifest)
        .unwrap()
        .lines()
        .find_map(|line| {
            line.split_once("  ")
                .and_then(|(digest, file)| (file == name).then_some(digest))
        })
        .unwrap_or_else(|| panic!("capture file is not sealed: {name}"));
    let bytes = fs::read(folder.join(name)).unwrap();
    assert_eq!(sha(&bytes), digest, "historical capture changed: {name}");
    bytes
}
fn options(temp: &Path, checker_present: bool) -> Options {
    let private = temp.join("source-root");
    for name in SOURCES {
        let target = private.join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(root().join(name), target).unwrap();
    }
    let historical = root().join(HISTORICAL);
    match fs::symlink_metadata(&historical) {
        Ok(metadata) => {
            assert!(metadata.file_type().is_file());
            assert_eq!(
                sha(&fs::read(&historical).unwrap()),
                "f0d0867a9481a698df8da04d472bdf2e2d6aa77da2a3ae29fba08aae74d5a836"
            );
            if checker_present {
                fs::copy(historical, private.join(HISTORICAL)).unwrap();
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            assert!(private.join(HISTORICAL).parent().unwrap().is_dir());
        }
        Err(error) => panic!("historical checker admission: {error}"),
    }
    let source = temp.join("private-source-probe");
    let binary = temp.join("private-native-probe");
    fs::write(&source, b"private source probe identity").unwrap();
    fs::write(&binary, b"private native probe identity").unwrap();
    Options {
        root: private,
        source,
        binary,
        output: temp.join("output"),
    }
}
struct OriginalTranscripts {
    calls: usize,
}
impl Runner for OriginalTranscripts {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        assert_eq!(spec.timeout, Duration::from_secs(90));
        let name = match self.calls {
            0 => "car-params.stdout",
            1 => "source.stdout",
            2 => "native.stdout",
            _ => return Err("unexpected fourth safety phase".into()),
        };
        self.calls += 1;
        Ok(Captured {
            code: 0,
            stdout: sealed(name),
            stderr: Vec::new(),
        })
    }
}
fn completed(options: &Options) -> padding::Completed {
    let mut runner = OriginalTranscripts { calls: 0 };
    let result = padding::execute(
        options,
        &mut runner,
        &AtomicBool::new(false),
        &mut Vec::new(),
    )
    .unwrap();
    assert_eq!(runner.calls, 3);
    assert!(options.output.join("report.json").exists());
    result
}
#[test]
fn post_execute_native_mutation_must_fail_finish_and_remove_owned_report() {
    let temp = tempfile::tempdir().unwrap();
    let options = options(temp.path(), true); // Old edb42bf0 requires the original checker pin.
    let result = completed(&options);
    fs::write(&options.binary, b"mutated after execute returned").unwrap();
    assert!(
        result.finish(&AtomicBool::new(false)).is_err(),
        "successful finalization accepted a mutated private native probe after execute returned"
    );
    assert!(
        !options.output.join("report.json").exists(),
        "owned PASS report survived a failed finalization"
    );
}
#[test]
fn post_execute_source_mutation_must_fail_finish_and_remove_owned_report() {
    let temp = tempfile::tempdir().unwrap();
    let options = options(temp.path(), true);
    let result = completed(&options);
    fs::write(
        options.root.join("openpilot/common/params.h"),
        b"mutated surviving source pin",
    )
    .unwrap();
    assert!(result.finish(&AtomicBool::new(false)).is_err());
    assert!(!options.output.join("report.json").exists());
}
#[test]
fn post_execute_absent_checker_reappearance_must_fail_finish() {
    let temp = tempfile::tempdir().unwrap();
    let options = options(temp.path(), false);
    let result = completed(&options);
    fs::write(options.root.join(HISTORICAL), b"unexpected reappearance").unwrap();
    let error = result.finish(&AtomicBool::new(false)).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("historical safety-padding checker presence changed"),
        "wrong finalization error: {error}"
    );
    assert!(!options.output.join("report.json").exists());
}
#[test]
fn post_execute_drift_never_removes_foreign_replacement_receipt() {
    let temp = tempfile::tempdir().unwrap();
    let options = options(temp.path(), true);
    let result = completed(&options);
    let report = options.output.join("report.json");
    fs::remove_file(&report).unwrap();
    fs::write(&report, b"foreign replacement").unwrap();
    fs::write(&options.source, b"source probe changed after execute").unwrap();
    assert!(result.finish(&AtomicBool::new(false)).is_err());
    assert_eq!(fs::read(report).unwrap(), b"foreign replacement");
}
