use openpilot_dev_tools::{
    plot_samples_check::check,
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::Path, time::Duration};
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn fixed() -> Value {
    let mut plain = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("fixtures/plot-samples/original.json.gz").as_slice(),
    )
    .read_to_string(&mut plain)
    .unwrap();
    serde_json::from_str::<Value>(&plain).unwrap()["fixed"].clone()
}
struct Peer {
    mode: String,
    response: Value,
    input: String,
    calls: usize,
}
impl Runner for Peer {
    fn run(&mut self, command: &CommandSpec) -> Result<Captured> {
        self.calls += 1;
        assert_eq!(
            command.effective_argv(),
            vec![std::ffi::OsString::from("relative/../candidate")]
        );
        assert_eq!(command.timeout, Duration::MAX);
        assert_eq!(command.input.as_deref(), Some(self.input.as_bytes()));
        Ok(Captured {
            code: if self.mode == "nonzero" { 7 } else { 0 },
            stdout: self.response["stdout"]
                .as_str()
                .unwrap_or("partial")
                .as_bytes()
                .to_vec(),
            stderr: self.response["stderr"]
                .as_str()
                .unwrap_or("failure")
                .as_bytes()
                .to_vec(),
        })
    }
}
#[test]
fn independent_original_process_cardinality_comparison_and_partial_artifacts() {
    let corpus: Value =
        serde_json::from_str(include_str!("fixtures/plot-samples/checker-original.json")).unwrap();
    let fixed = fixed();
    let mut raw = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("fixtures/plot-samples/checker-responses.json.gz").as_slice(),
    )
    .read_to_string(&mut raw)
    .unwrap();
    let responses: Value = serde_json::from_str(&raw).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for row in corpus["cases"].as_array().unwrap() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("output");
        let mode = row["mode"].as_str().unwrap();
        if mode.ends_with("-dir") {
            fs::create_dir_all(output.join(match mode {
                "source-dir" => "source.json",
                "native-dir" => "native.json",
                "stderr-dir" => "native.stderr",
                _ => "result.json",
            }))
            .unwrap();
        }
        let mut peer = Peer {
            mode: mode.into(),
            response: responses["cases"][mode]
                .as_u64()
                .map(|i| responses["values"][i as usize].clone())
                .unwrap_or(Value::Null),
            input: fixed["input"].as_str().unwrap().into(),
            calls: 0,
        };
        let mut printed = vec![];
        let result = check(
            &root,
            Path::new("./relative/../candidate"),
            &output,
            &mut peer,
            &mut printed,
        );
        assert_eq!(result.is_ok(), row["error"].is_null(), "{mode}");
        assert_eq!(peer.calls, row["calls"].as_array().unwrap().len(), "{mode}");
        assert_eq!(
            String::from_utf8(printed).unwrap(),
            row["stdout"].as_str().unwrap(),
            "{mode}"
        );
        let actual = fs::read_dir(&output)
            .unwrap()
            .filter_map(|e| {
                let p = e.unwrap().path();
                p.is_file().then(|| {
                    let b = fs::read(&p).unwrap();
                    (
                        p.file_name().unwrap().to_str().unwrap().to_owned(),
                        serde_json::json!({"sha256":sha(&b),"bytes":b.len()}),
                    )
                })
            })
            .collect::<serde_json::Map<_, _>>();
        assert_eq!(Value::Object(actual), row["files"], "{mode}");
    }
}

#[test]
fn live_source_and_reintroduced_helpers_are_checked_before_candidate_execution() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let manifest: Value =
        serde_json::from_str(include_str!("fixtures/plot-samples/manifest.json")).unwrap();
    for name in manifest["sources"].as_object().unwrap().keys() {
        let temporary = tempfile::tempdir().unwrap();
        for path in manifest["sources"].as_object().unwrap().keys() {
            let to = temporary.path().join(path);
            fs::create_dir_all(to.parent().unwrap()).unwrap();
            if root.join(path).exists() {
                fs::copy(root.join(path), &to).unwrap();
            }
        }
        fs::write(temporary.path().join(name), "changed source").unwrap();
        let mut peer = Peer {
            mode: "success".into(),
            response: Value::Null,
            input: String::new(),
            calls: 0,
        };
        let output = temporary.path().join("output");
        assert!(check(
            temporary.path(),
            Path::new("candidate"),
            &output,
            &mut peer,
            &mut vec![]
        )
        .is_err());
        assert_eq!(peer.calls, 0);
        assert_eq!(fs::read_dir(output).unwrap().count(), 0);
    }
}

#[test]
fn ui_job_builds_and_runs_native_sampling_checker_with_the_real_candidate() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let workflow = openpilot_dev_tools::ci_policy::parse(
        &fs::read_to_string(root.join(".github/workflows/rust.yml")).unwrap(),
    )
    .unwrap();
    let steps = workflow["jobs"]["ui-runtime"]["steps"].as_array().unwrap();
    let commands = steps
        .iter()
        .filter_map(|v| v["run"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(commands.contains("--bin check-ui-plot-samples"));
    assert!(commands.contains("--example plot_samples"));
    assert!(commands.contains("--test plot_samples_source --test plot_samples_check"));
    let caller = fs::read_to_string(root.join("rust/tools/check_ui_ci.py")).unwrap();
    assert!(caller.contains("('plot-samples', 'plot_samples')"));
}

#[test]
fn cli_help_argument_errors_and_generic_reference_stdin_are_real_native_processes() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    for args in [
        vec!["--help"],
        vec![],
        vec!["--binary", "x"],
        vec!["--binary", "x", "--output", "y", "--unknown"],
    ] {
        let dir = tempfile::tempdir().unwrap();
        let result = Command::new(env!("CARGO_BIN_EXE_check-ui-plot-samples"))
            .args(&args)
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert_eq!(
            result.status.code(),
            Some(if args == ["--help"] { 0 } else { 2 })
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    }
    let mut child = Command::new(env!("CARGO_BIN_EXE_plot-samples-reference"))
        .env("PATH", "/nonexistent")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"[{\"now\":0,\"values\":[1,2,3],\"history\":true}]")
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let value: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(
        value[0]["sample"]["latest"],
        serde_json::json!([1.0, 2.0, 3.0])
    );
}

#[test]
fn original_fixture_hashes_and_unicode_printability_pin_are_intact() {
    let manifest: Value =
        serde_json::from_str(include_str!("fixtures/plot-samples/manifest.json")).unwrap();
    for (key, bytes) in [
        (
            "checker_capture_sha256",
            include_bytes!("fixtures/plot-samples/checker-original.json").as_slice(),
        ),
        (
            "checker_responses_gzip_sha256",
            include_bytes!("fixtures/plot-samples/checker-responses.json.gz").as_slice(),
        ),
        (
            "repr_nonprintable_sha256",
            include_bytes!("fixtures/plot-samples/repr-nonprintable.json").as_slice(),
        ),
    ] {
        assert_eq!(manifest[key].as_str(), Some(sha(bytes).as_str()));
    }
}
