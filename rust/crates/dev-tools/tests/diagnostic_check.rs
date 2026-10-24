use openpilot_dev_tools::{
    diagnostic_check::{check, Kind},
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
fn sha(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
fn inflate(p: &Path) -> Vec<u8> {
    let mut b = vec![];
    flate2::read::GzDecoder::new(fs::File::open(p).unwrap())
        .read_to_end(&mut b)
        .unwrap();
    b
}
struct Peer {
    fixtures: PathBuf,
    rows: Vec<Value>,
    index: usize,
}
impl Runner for Peer {
    fn run(&mut self, s: &CommandSpec) -> Result<Captured> {
        let row = &self.rows[self.index];
        self.index += 1;
        assert_eq!(sha(s.input.as_ref().unwrap()), row["input_sha256"]);
        assert!(s.effective_argv()[0]
            .as_os_str()
            .to_string_lossy()
            .ends_with("/controlled-native"));
        assert!(Path::new(&s.effective_argv()[0]).is_absolute());
        assert_eq!(s.timeout, Duration::MAX);
        assert_eq!(s.stdout_limit, usize::MAX);
        assert_eq!(s.stderr_limit, usize::MAX);
        assert!(s.effective_env().is_empty());
        assert!(s.cwd.is_none());
        if row["spawn_error"] == true {
            return Err("controlled spawn failure".into());
        }
        Ok(Captured {
            code: row["code"].as_i64().unwrap() as i32,
            stdout: inflate(
                &self
                    .fixtures
                    .join("replies")
                    .join(format!("{}.gz", row["reply_sha256"].as_str().unwrap())),
            ),
            stderr: if row["stderr_hex"] == "ff" {
                vec![255]
            } else {
                vec![]
            },
        })
    }
}
fn original_cases(communication: bool) {
    let fixtures =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/diagnostic-check");
    let report: Value =
        serde_json::from_slice(&fs::read(fixtures.join("original-cases.json")).unwrap()).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for row in report["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| (row["family"] == "communication") == communication)
    {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("out");
        if row["mode"] == "output-exists" {
            fs::create_dir(&output).unwrap()
        }
        let mut peer = Peer {
            fixtures: fixtures.clone(),
            rows: row["calls"].as_array().unwrap().clone(),
            index: 0,
        };
        let kind = if row["family"] == "diagnostics" {
            Kind::Runtime
        } else if row["family"] == "communication" {
            Kind::Communication
        } else {
            Kind::ModelTiming
        };
        let result = check(
            &root,
            Path::new("./controlled-native"),
            &output,
            kind,
            &mut peer,
        );
        assert_eq!(
            result.is_ok(),
            row["exit"] == 0,
            "{} {} {result:?}",
            row["family"],
            row["mode"]
        );
        assert_eq!(
            peer.index,
            peer.rows.len(),
            "{} {}",
            row["family"],
            row["mode"]
        );
        if let Ok(stdout) = result {
            assert_eq!(stdout, row["stdout"])
        }
        let actual = fs::read_dir(&output)
            .unwrap()
            .map(|p| {
                let p = p.unwrap();
                (
                    p.file_name().into_string().unwrap(),
                    sha(&fs::read(p.path()).unwrap()),
                )
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        let expected = row["files"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v["sha256"].as_str().unwrap().to_owned()))
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(actual, expected, "{} {}", row["family"], row["mode"]);
    }
}
#[test]
fn complete_original_types_key_order_float_bits_and_failure_artifacts() {
    original_cases(false);
}
#[test]
fn complete_original_communication_snapshots_and_failure_artifacts() {
    original_cases(true);
}
#[test]
fn independently_captured_comparator_scalars() {
    use openpilot_dev_tools::diagnostic_check::compare_exact;
    use openpilot_log_format::JsonValue;
    for (a, b, accepted) in [
        ("0.0", "0.0", true),
        ("0.0", "-0.0", false),
        ("NaN", "NaN", true),
        ("1.0", "1", false),
        ("true", "1", false),
        ("{\"a\": 1, \"b\": 2}", "{\"b\": 2, \"a\": 1}", false),
        ("Infinity", "Infinity", true),
    ] {
        assert_eq!(
            compare_exact(&JsonValue::parse(a).unwrap(), &JsonValue::parse(b).unwrap()).is_ok(),
            accepted,
            "{a} / {b}"
        );
    }
}
struct Never;
impl Runner for Never {
    fn run(&mut self, _: &CommandSpec) -> Result<Captured> {
        panic!("candidate ran after source/dependency drift")
    }
}
#[test]
fn original_source_and_capture_dependency_drift_fail_closed() {
    let fixtures =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/diagnostic-check");
    let manifest: Value =
        serde_json::from_slice(&fs::read(fixtures.join("manifest.json")).unwrap()).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for kind in [Kind::Runtime, Kind::ModelTiming, Kind::Communication] {
        let temp = tempfile::tempdir().unwrap();
        let key = match kind {
            Kind::Runtime => "runtime",
            Kind::ModelTiming => "model_timing",
            Kind::Communication => "communication",
        };
        for (path, _) in manifest["sources"][key].as_object().unwrap() {
            let dest = temp.path().join(path);
            fs::create_dir_all(dest.parent().unwrap()).unwrap();
            fs::copy(root.join(path), dest).unwrap();
        }
        fs::copy(root.join("uv.lock"), temp.path().join("uv.lock")).unwrap();
        fs::write(
            temp.path().join("openpilot/common/runtime_diagnostics.py"),
            "drift",
        )
        .unwrap();
        let output = temp.path().join("out");
        assert!(
            check(temp.path(), Path::new("never"), &output, kind, &mut Never)
                .unwrap_err()
                .to_string()
                .contains("source changed")
        );
        assert!(!output.exists());
    }
    let temp = tempfile::tempdir().unwrap();
    for (path, _) in manifest["sources"]["model_timing"].as_object().unwrap() {
        let dest = temp.path().join(path);
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::copy(root.join(path), dest).unwrap();
    }
    fs::write(temp.path().join("uv.lock"), "package=[]").unwrap();
    assert!(check(
        temp.path(),
        Path::new("never"),
        &temp.path().join("out"),
        Kind::ModelTiming,
        &mut Never
    )
    .unwrap_err()
    .to_string()
    .contains("pytest source changed"));
    let temp = tempfile::tempdir().unwrap();
    for path in manifest["sources"]["communication"]
        .as_object()
        .unwrap()
        .keys()
    {
        let dest = temp.path().join(path);
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::copy(root.join(path), dest).unwrap();
    }
    fs::write(temp.path().join("uv.lock"), "package=[]").unwrap();
    assert!(check(
        temp.path(),
        Path::new("never"),
        &temp.path().join("out"),
        Kind::Communication,
        &mut Never
    )
    .unwrap_err()
    .to_string()
    .contains("communication dependencies changed"));
}
