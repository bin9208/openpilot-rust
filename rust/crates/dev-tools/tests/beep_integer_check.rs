use openpilot_dev_tools::{
    beep_integer_check::check,
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn unpack(bytes: &[u8]) -> Value {
    let mut data = vec![];
    flate2::read::GzDecoder::new(bytes)
        .read_to_end(&mut data)
        .unwrap();
    serde_json::from_slice(&data).unwrap()
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
// The native source lane deliberately replaces the historical Cython runner.
// Only provenance labels change; all captured values, diagnostics, comparison
// order, failure behavior and report layout remain original expectations.
fn migrated(text: &str) -> String {
    text.replace(
        "\"implementation\": \"python\"",
        "\"implementation\": \"source-reference\"",
    )
    .replace(
        "real Cython/native integer cases",
        "original C++ Params/native integer cases",
    )
}
fn reference(root: &Path, temporary: &Path) -> PathBuf {
    let binary = temporary.join("reference");
    fs::write(&binary, b"test source executable").unwrap();
    let mut sources = serde_json::Map::new();
    for name in [
        "rust/tools/beep_integer_reference.cc",
        "openpilot/common/params.cc",
        "openpilot/common/params.h",
        "openpilot/common/util.cc",
    ] {
        sources.insert(
            name.into(),
            Value::String(sha(&fs::read(root.join(name)).unwrap())),
        );
    }
    fs::write(binary.with_extension("provenance.json"),serde_json::to_vec(&serde_json::json!({"kind":"beep-integer-original-params","binary_sha256":sha(&fs::read(&binary).unwrap()),"sources":sources})).unwrap()).unwrap();
    binary
}
struct Peer {
    row: Value,
    calls: usize,
    directories: Vec<PathBuf>,
}
impl Runner for Peer {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        let call = self.row["calls"][self.calls].clone();
        self.calls += 1;
        assert_eq!(spec.timeout, Duration::from_secs(5));
        assert_eq!(spec.stdout_limit, usize::MAX);
        assert_eq!(spec.stderr_limit, usize::MAX);
        let env = spec.effective_env();
        let directory = PathBuf::from(env.get(std::ffi::OsStr::new("PARAMS_ROOT")).unwrap());
        let prefix = env.get(std::ffi::OsStr::new("OPENPILOT_PREFIX")).unwrap();
        let bytes = fs::read(directory.join(prefix).join("SoundVolumeAdjust")).unwrap();
        assert_eq!(serde_json::json!(bytes), call["bytes"]);
        assert!(directory.join("bin").is_dir());
        self.directories.push(directory.clone());
        let argv = spec.effective_argv();
        if call["lane"] == "python" {
            assert_eq!(argv.len(), 2);
            assert_eq!(Path::new(&argv[1]), directory);
            assert!(spec.input.is_none());
        } else {
            assert_eq!(argv.len(), 1);
            let config: Value = serde_json::from_slice(spec.input.as_ref().unwrap()).unwrap();
            assert_eq!(config["mode"], "integer");
            assert_eq!(config["bytes"], call["bytes"]);
            assert_eq!(config["root"], directory.to_str().unwrap());
        }
        if call.get("code").is_none() {
            return Err("controlled original subprocess failure".into());
        }
        Ok(Captured {
            code: call["code"].as_i64().unwrap() as i32,
            stdout: call["stdout"].as_str().unwrap().as_bytes().to_vec(),
            stderr: call["stderr"].as_str().unwrap().as_bytes().to_vec(),
        })
    }
}
#[test]
fn original_comparison_failure_order_summary_bytes_and_owned_cleanup() {
    let rows = unpack(include_bytes!("fixtures/beep-integer/failures.json.gz"));
    for row in rows.as_array().unwrap() {
        let temporary = tempfile::tempdir().unwrap();
        let reference = reference(&root(), temporary.path());
        let output = temporary.path().join("output");
        fs::create_dir(&output).unwrap();
        if row["blocked"] == true {
            fs::create_dir(output.join("summary.json")).unwrap();
        } else {
            fs::write(output.join("summary.json"), b"keep").unwrap();
        }
        let mut peer = Peer {
            row: row.clone(),
            calls: 0,
            directories: vec![],
        };
        let mut printed = vec![];
        let result = check(
            &root(),
            Path::new("candidate"),
            &reference,
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
            row["calls"].as_array().unwrap().len(),
            "{}",
            row["mode"]
        );
        assert_eq!(
            String::from_utf8(printed).unwrap(),
            migrated(row["stdout"].as_str().unwrap())
        );
        if let Some(summary) = row["summary"].as_str() {
            assert_eq!(
                fs::read_to_string(output.join("summary.json")).unwrap(),
                migrated(summary),
                "{}",
                row["mode"]
            );
        } else {
            assert!(output.join("summary.json").is_dir());
        }
        assert!(
            peer.directories.iter().all(|path| !path.exists()),
            "owned Params stores survived {}",
            row["mode"]
        );
    }
}
#[test]
fn changed_or_missing_source_and_substituted_native_reference_fail_before_spawn() {
    let temporary = tempfile::tempdir().unwrap();
    let reference = reference(&root(), temporary.path());
    let row = unpack(include_bytes!("fixtures/beep-integer/failures.json.gz"))[0].clone();
    let mut peer = Peer {
        row,
        calls: 0,
        directories: vec![],
    };
    let missing = temporary.path().join("missing");
    fs::create_dir(&missing).unwrap();
    assert!(check(
        &missing,
        Path::new("candidate"),
        &reference,
        &temporary.path().join("output"),
        &mut peer,
        &mut vec![]
    )
    .is_err());
    assert_eq!(peer.calls, 0);
    fs::write(&reference, b"substituted executable").unwrap();
    assert!(check(
        &root(),
        Path::new("candidate"),
        &reference,
        &temporary.path().join("output"),
        &mut peer,
        &mut vec![]
    )
    .is_err());
    assert_eq!(peer.calls, 0);
}

#[test]
fn migrated_cli_preserves_original_help_errors_and_has_no_side_effects() {
    let rows: Value = serde_json::from_str(include_str!("fixtures/beep-integer/cli.json")).unwrap();
    for row in rows.as_array().unwrap() {
        let temporary = tempfile::tempdir().unwrap();
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_check-beep-integer"))
            .args(
                row["native_args"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap()),
            )
            .current_dir(temporary.path())
            .output()
            .unwrap();
        assert_eq!(
            result.status.code(),
            row["exit"].as_i64().map(|n| n as i32),
            "{:?}",
            row["original_args"]
        );
        assert_eq!(!result.stdout.is_empty(), row["help"] == true);
        assert_eq!(fs::read_dir(temporary.path()).unwrap().count(), 0);
    }
    // The Cython module input was intentionally replaced by a native reference.
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_check-beep-integer"))
        .args([
            "--binary",
            "candidate",
            "--binding",
            "cython.so",
            "--output",
            "output",
        ])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
}

#[test]
fn source_builder_argument_help_and_errors_do_not_start_a_build() {
    for (args, code) in [
        (vec!["--help"], 0),
        (vec![], 2),
        (vec!["--output"], 2),
        (vec!["--unknown"], 2),
        (vec!["--capnp-prefix", "prefix"], 2),
    ] {
        let temporary = tempfile::tempdir().unwrap();
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_build-beep-integer-reference"))
            .args(args)
            .current_dir(temporary.path())
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(code));
        assert_eq!(fs::read_dir(temporary.path()).unwrap().count(), 0);
    }
}

#[test]
fn output_mkdir_failure_precedes_reference_open_and_no_child_is_started() {
    let temporary = tempfile::tempdir().unwrap();
    let output = temporary.path().join("output");
    fs::write(&output, b"foreign output").unwrap();
    let row = unpack(include_bytes!("fixtures/beep-integer/failures.json.gz"))[0].clone();
    let mut peer = Peer {
        row,
        calls: 0,
        directories: vec![],
    };
    let error = check(
        &root(),
        Path::new("candidate"),
        Path::new("missing-reference"),
        &output,
        &mut peer,
        &mut vec![],
    )
    .unwrap_err();
    assert!(error
        .downcast_ref::<std::io::Error>()
        .is_some_and(|error| error.kind() == std::io::ErrorKind::AlreadyExists));
    assert_eq!(peer.calls, 0);
    assert_eq!(fs::read(output).unwrap(), b"foreign output");
}

struct BuilderPeer {
    fail: usize,
    calls: usize,
}
impl Runner for BuilderPeer {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        self.calls += 1;
        if self.calls != self.fail {
            let argv = spec.effective_argv();
            if argv.first().is_some_and(|s| s == "g++") {
                fs::write(argv.last().unwrap(), b"successful test compiler output")?;
            }
        }
        Ok(Captured {
            code: if self.calls == self.fail { 7 } else { 0 },
            stdout: vec![],
            stderr: vec![],
        })
    }
}
impl openpilot_dev_tools::params_reference_check::ParamsRunner for BuilderPeer {
    fn locked_put(&mut self, _writer: &Path, _store: &Path) -> Result<()> {
        panic!("source build must not run lock checks")
    }
}
#[test]
fn each_build_failure_leaves_no_success_receipt_and_existing_output_is_preserved() {
    use openpilot_dev_tools::beep_integer_check::build_reference;
    for fail in [1, 2, 3] {
        let temporary = tempfile::tempdir().unwrap();
        let output = temporary.path().join("build");
        let mut peer = BuilderPeer { fail, calls: 0 };
        assert!(build_reference(&root(), &output, None, &mut peer).is_err());
        assert_eq!(peer.calls, fail);
        assert!(!output
            .join("beep-integer-reference.provenance.json")
            .exists());
        let count = peer.calls;
        assert!(build_reference(&root(), &output, None, &mut peer).is_err());
        assert_eq!(peer.calls, count, "failed output must not be reused");
    }
    let temporary = tempfile::tempdir().unwrap();
    let output = temporary.path().join("build");
    fs::create_dir(&output).unwrap();
    fs::write(
        output.join("beep-integer-reference.provenance.json"),
        b"foreign receipt",
    )
    .unwrap();
    let mut peer = BuilderPeer { fail: 0, calls: 0 };
    assert!(build_reference(&root(), &output, None, &mut peer).is_err());
    assert_eq!(peer.calls, 0);
    assert_eq!(
        fs::read(output.join("beep-integer-reference.provenance.json")).unwrap(),
        b"foreign receipt"
    );
}
