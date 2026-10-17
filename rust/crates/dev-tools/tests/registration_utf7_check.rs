use openpilot_dev_tools::{
    qa_process::{Captured, CommandSpec, Runner},
    registration_utf7_check::check,
    seal::Result,
};
use serde_json::Value;
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
fn corpus() -> Value {
    serde_json::from_str(include_str!("fixtures/registration-utf7/original.json")).unwrap()
}
struct Peer {
    response: Value,
    input: String,
    calls: usize,
}
impl Runner for Peer {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        self.calls += 1;
        assert_eq!(spec.input.as_deref(), Some(self.input.as_bytes()));
        assert_eq!(spec.timeout, Duration::MAX);
        let args = spec.effective_argv();
        assert_eq!(args[0], "runner");
        assert_eq!(args[1], "-q");
        assert!(Path::new(&args[2]).is_absolute());
        Ok(Captured {
            code: self.response["code"].as_i64().unwrap() as i32,
            stdout: self.response["stdout"]
                .as_str()
                .map_or(vec![255], |s| s.as_bytes().to_vec()),
            stderr: self.response["stderr"]
                .as_str()
                .map_or(vec![255], |s| s.as_bytes().to_vec()),
        })
    }
}
#[test]
fn original_utf7_corpus_and_process_json_equality_artifact_boundaries() {
    let original = corpus();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for case in original["cases"].as_array().unwrap() {
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("candidate");
        fs::write(&binary, b"original-fixture-executable-identity").unwrap();
        let mode = case["mode"].as_str().unwrap();
        let out = if mode == "report-missing-parent" {
            dir.path().join("missing/output.json")
        } else {
            dir.path().join("output.json")
        };
        if mode == "report-dir" {
            fs::create_dir(&out).unwrap();
        }
        let mut peer = Peer {
            response: original["responses"][mode].clone(),
            input: original["fixed"]["input"].as_str().unwrap().into(),
            calls: 0,
        };
        let mut printed = vec![];
        let result = check(
            &root,
            &binary,
            &out,
            &[OsString::from("runner"), OsString::from("-q")],
            &mut peer,
            &mut printed,
        );
        assert_eq!(
            result.is_ok(),
            case["error"].is_null(),
            "{mode}: {result:?}"
        );
        assert_eq!(peer.calls, 1);
        assert_eq!(out.is_file(), !case["report"].is_null(), "{mode}");
        if out.is_file() {
            let mut report: Value = serde_json::from_slice(&fs::read(&out).unwrap()).unwrap();
            report["argv"] = serde_json::json!(["runner", "-q", "BINARY"]);
            assert_eq!(report, case["report"], "{mode}");
        }
        assert_eq!(
            String::from_utf8(printed).unwrap(),
            case["stdout"].as_str().unwrap(),
            "{mode}"
        );
    }
}
#[test]
fn modified_reintroduced_checker_fails_before_candidate() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rust/tools/check_registration_utf7.py");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, b"changed").unwrap();
    let original = corpus();
    let mut peer = Peer {
        response: original["responses"]["exact"].clone(),
        input: String::new(),
        calls: 0,
    };
    assert!(check(
        dir.path(),
        &PathBuf::from("missing"),
        &dir.path().join("output"),
        &[],
        &mut peer,
        &mut vec![]
    )
    .is_err());
    assert_eq!(peer.calls, 0);
}
#[test]
fn native_cli_argument_and_runner_contract() {
    let binary = env!("CARGO_BIN_EXE_check-registration-utf7");
    assert!(std::process::Command::new(binary)
        .arg("--help")
        .status()
        .unwrap()
        .success());
    for args in [
        vec![],
        vec!["one"],
        vec!["a", "b", "--runner", "-q"],
        vec!["a", "b", "--unknown"],
    ] {
        let result = std::process::Command::new(binary)
            .args(args)
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(2));
    }
}
#[test]
fn actual_original_cli_report_recursion_boundary_preserves_no_report_on_failure() {
    // Independent real CLI probes: reviewer results.json/results993.json. Original
    // JSON encoding allows992 nested candidate arrays, but993 fails before writing.
    let original = corpus();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let normal = original["responses"]["exact"]["stdout"].as_str().unwrap();
    let (_, tail) = normal.split_once('\n').unwrap();
    for depth in [992, 993, 994, 1000] {
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("candidate");
        fs::write(&binary, b"fixture").unwrap();
        let out = dir.path().join("output.json");
        let mut response = original["responses"]["exact"].clone();
        response["stdout"] = Value::String(format!(
            "{}{}\n{tail}",
            "[".repeat(depth),
            "]".repeat(depth)
        ));
        let mut peer = Peer {
            response,
            input: original["fixed"]["input"].as_str().unwrap().into(),
            calls: 0,
        };
        let result = check(
            &root,
            &binary,
            &out,
            &["runner".into(), "-q".into()],
            &mut peer,
            &mut vec![],
        );
        assert!(result.is_err());
        assert_eq!(out.exists(), depth == 992, "depth{depth}");
    }
}
