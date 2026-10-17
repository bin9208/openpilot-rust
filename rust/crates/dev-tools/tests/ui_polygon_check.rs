use openpilot_dev_tools::{
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
    ui_polygon_check::check,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn blob(fixtures: &Path, key: &str) -> Vec<u8> {
    let mut bytes = vec![];
    flate2::read::GzDecoder::new(
        fs::File::open(fixtures.join("checker-blobs").join(format!("{key}.gz"))).unwrap(),
    )
    .read_to_end(&mut bytes)
    .unwrap();
    assert_eq!(sha(&bytes), key);
    bytes
}
struct Peer<'a> {
    row: &'a Value,
    fixtures: PathBuf,
    root: PathBuf,
    output: PathBuf,
    calls: usize,
    binary: PathBuf,
}
impl Runner for Peer<'_> {
    fn run_to_file(&mut self, spec: &CommandSpec, log: &Path) -> Result<Captured> {
        // The independent original harness writes this log before its mocked
        // spawn raises. Preserve those pre-error bytes as well as successes.
        fs::write(log, b"log-\xff\n")?;
        let mut spec = spec.clone();
        spec.merge_output = true;
        self.run(&spec)
    }
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        let captured = &self.row["calls"][self.calls];
        self.calls += 1;
        let lane = captured["lane"].as_str().unwrap();
        let name = captured["scene"].as_str().unwrap();
        let mode = self.row["mode"].as_str().unwrap();
        assert_eq!(spec.timeout, Duration::MAX);
        assert_eq!(spec.stdout_limit, usize::MAX);
        assert_eq!(spec.stderr_limit, usize::MAX);
        assert!(spec.inherit_stdin);
        assert!(spec.merge_output);
        let mut argv = vec![if lane == "source" {
            std::ffi::OsString::from("reference")
        } else {
            self.binary.clone().into_os_string()
        }];
        if lane == "native" {
            argv.push(self.root.clone().into_os_string());
        }
        argv.push(self.output.join(format!("{name}.json")).into_os_string());
        argv.push(
            self.output
                .join(format!("{lane}-{name}.png"))
                .into_os_string(),
        );
        assert_eq!(spec.effective_argv(), argv);
        let env = spec.effective_env();
        assert_eq!(env.get(std::ffi::OsStr::new("DISPLAY")).unwrap(), ":owned");
        assert_eq!(env.get(std::ffi::OsStr::new("OFFSCREEN")).unwrap(), "1");
        assert_eq!(
            env.get(std::ffi::OsStr::new("PYTHONPATH")).unwrap(),
            self.root.as_os_str()
        );
        if mode == "native-spawn" && lane == "native" {
            return Err("controlled missing executable".into());
        }
        for extension in ["png", "json"] {
            let filename = format!("{lane}-{name}.{extension}");
            if let Some(key) = self.row["files"][&filename].as_str() {
                fs::write(self.output.join(filename), blob(&self.fixtures, key))?;
            }
        }
        Ok(Captured {
            code: if mode == format!("{lane}-fail") { 7 } else { 0 },
            stdout: b"log-\xff\n".to_vec(),
            stderr: vec![],
        })
    }
}
fn all_files(root: &Path) -> std::collections::BTreeMap<PathBuf, String> {
    let mut result = std::collections::BTreeMap::new();
    for entry in fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_file() {
            result.insert(
                entry.file_name().into(),
                sha(&fs::read(entry.path()).unwrap()),
            );
        }
    }
    result
}
#[test]
fn unchanged_whole_checker_32_cases_preserve_pixels_python_states_output_bytes_and_failure_artifacts(
) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ui-polygon");
    // Captured argv contains real Python surrogateescape strings. Retain those
    // in the provenance fixture; project only the fields this test consumes.
    use openpilot_log_format::{JsonValue, JsonView};
    let original =
        JsonValue::parse(include_str!("fixtures/ui-polygon/checker-original.json")).unwrap();
    let JsonView::Array(rows) = original.get("cases").unwrap().view() else {
        panic!("cases array")
    };
    for original in rows {
        let JsonView::Array(calls) = original.get("calls").unwrap().view() else {
            panic!("calls array")
        };
        let calls = calls.iter().map(|call| serde_json::json!({"lane":call.get("lane").unwrap().to_utf8().unwrap(),"scene":call.get("scene").unwrap().to_utf8().unwrap()})).collect::<Vec<_>>();
        let row = serde_json::json!({
            "mode":original.get("mode").unwrap().to_utf8().unwrap(),
            "calls":calls,"stdout":original.get("stdout").unwrap().to_utf8().unwrap(),
            "failure":if matches!(original.get("failure").unwrap().view(),JsonView::Null) {Value::Null}else{serde_json::json!({})},
            "files":serde_json::from_str::<Value>(&original.get("files").unwrap().to_json().unwrap()).unwrap()
        });
        let temp = tempfile::tempdir().unwrap();
        let mode = row["mode"].as_str().unwrap();
        let output = temp.path().join(if mode == "raw" {
            std::ffi::OsString::from_vec(b"output-\xfe".to_vec())
        } else {
            "out".into()
        });
        if matches!(mode, "existing-output" | "results-directory") {
            fs::create_dir(&output).unwrap();
            fs::write(output.join("sentinel"), b"keep").unwrap();
            if mode == "results-directory" {
                fs::create_dir(output.join("results.json")).unwrap();
            }
        }
        let binary = if mode == "raw" {
            PathBuf::from(std::ffi::OsString::from_vec(b"candidate-\xff".to_vec()))
        } else {
            PathBuf::from("candidate")
        };
        let mut peer = Peer {
            row: &row,
            fixtures: fixtures.clone(),
            root: root.clone(),
            output: output.clone(),
            calls: 0,
            binary: binary.clone(),
        };
        let mut printed = vec![];
        let result = check(
            &root,
            &binary,
            Path::new("reference"),
            &output,
            std::ffi::OsStr::new(":owned"),
            &mut peer,
            &mut printed,
        );
        assert_eq!(
            result.is_ok(),
            row["failure"].is_null(),
            "{mode}: {result:?}"
        );
        assert_eq!(
            printed,
            row["stdout"].as_str().unwrap().as_bytes(),
            "{mode}"
        );
        assert_eq!(peer.calls, row["calls"].as_array().unwrap().len(), "{mode}");
        let expected = row["files"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(name, key)| (PathBuf::from(name), key.as_str().unwrap().to_owned()))
            .collect();
        assert_eq!(all_files(&output), expected, "{mode}");
    }
}
use std::os::unix::ffi::OsStringExt;

#[test]
fn original_checker_cli_help_and_argument_failures_preserve_exit_and_no_artifacts() {
    let captured: Value = serde_json::from_str(include_str!(
        "fixtures/ui-polygon/checker-cli-original.json"
    ))
    .unwrap();
    for row in captured["cases"].as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let observed = std::process::Command::new(env!("CARGO_BIN_EXE_check-ui-polygon"))
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
        assert_eq!(
            observed.status.code(),
            row["exit"].as_i64().map(|v| v as i32),
            "{row}"
        );
        assert_eq!(!observed.stdout.is_empty(), row["help"] == true, "{row}");
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0, "{row}");
    }
}

#[test]
fn reintroduced_checker_source_drift_fails_before_candidate_execution() {
    struct Never;
    impl Runner for Never {
        fn run(&mut self, _: &CommandSpec) -> Result<Captured> {
            panic!("source drift must fail before candidate execution")
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("rust/tools/check_ui_polygon.py");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    for source_bytes in [b"changed original checker".as_slice(), b""] {
        fs::write(&source, source_bytes).unwrap();
        let result = check(
            temp.path(),
            Path::new("candidate"),
            Path::new("reference"),
            &temp.path().join("output"),
            std::ffi::OsStr::new(":owned"),
            &mut Never,
            &mut vec![],
        );
        assert!(result.is_err());
        assert_eq!(fs::read_dir(temp.path().join("output")).unwrap().count(), 0);
    }
}
