#![cfg(feature = "lagd-tools-native")]
//! Original process kwargs, exact report bytes and partial failure effects.
use base64::{engine::general_purpose::STANDARD, Engine};
use openpilot_dev_tools::{
    lagd_check::{run, Kind, Options},
    qa_process::{CaptureError, Captured, CommandSpec, Runner},
    seal::Result,
};
use openpilot_log_format::JsonValue;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, ffi::OsString, fs, path::Path, time::Duration};
#[path = "support/lagd_expected.rs"]
mod expected;
#[path = "support/lagd_report.rs"]
mod report;

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
struct Peer<'a> {
    row: &'a Value,
    blobs: &'a Value,
    calls: usize,
}
impl Runner for Peer<'_> {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        let call = &self.row["calls"][self.calls];
        self.calls += 1;
        let expected_argv = call["argv"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| OsString::from(v.as_str().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(spec.effective_argv(), expected_argv);
        assert_eq!(
            spec.timeout,
            Duration::from_secs(call["timeout"].as_u64().unwrap())
        );
        assert_eq!(spec.stdout_limit, usize::MAX);
        assert_eq!(spec.stderr_limit, usize::MAX);
        assert!(
            !spec.inherit_output
                && !spec.inherit_stderr
                && !spec.inherit_stdin
                && !spec.merge_output
        );
        assert!(
            spec.cwd.is_none() && spec.effective_env().is_empty() && spec.env_remove.is_empty()
        );
        let input = std::str::from_utf8(spec.input.as_ref().unwrap()).unwrap();
        let input = if self.row["tool"] == "cache" {
            let parsed: Value = serde_json::from_str(input).unwrap();
            input.replace(parsed["root"].as_str().unwrap(), "@PARAMS_ROOT@")
        } else {
            input.to_owned()
        };
        assert_eq!(input.len() as u64, call["input"]["bytes"].as_u64().unwrap());
        assert_eq!(
            sha(input.as_bytes()),
            call["input"]["sha256"].as_str().unwrap()
        );
        let mode = self.row["case"].as_str().unwrap();
        if mode == "timeout" {
            return Err(CaptureError::Timeout {
                timeout: spec.timeout,
                stdout: b"partial-out".to_vec(),
                stderr: b"partial-err".to_vec(),
            }
            .into());
        }
        if mode == "exit-one"
            || (mode == "second-exit" && self.calls == 2)
            || (mode == "last-exit" && self.calls == self.row["calls"].as_array().unwrap().len())
        {
            return Ok(Captured {
                code: 7,
                stdout: b"native-out".to_vec(),
                stderr: b"native-err".to_vec(),
            });
        }
        if mode == "decode-failure" {
            return Ok(Captured {
                code: 0,
                stdout: vec![255],
                stderr: vec![],
            });
        }
        let read = |key: &str| {
            STANDARD
                .decode(
                    self.blobs[call[key]["sha256"].as_str().unwrap()]
                        .as_str()
                        .unwrap(),
                )
                .unwrap()
        };
        Ok(Captured {
            code: 0,
            stdout: read("stdout"),
            stderr: read("stderr"),
        })
    }
}
fn files(
    root: &Path,
    path: &Path,
    all: &mut BTreeMap<String, String>,
    directories: &mut Vec<String>,
) {
    if !path.is_dir() {
        return;
    }
    for entry in fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        let name = path
            .strip_prefix(root)
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        if path.is_dir() {
            directories.push(name);
            files(root, &path, all, directories);
        } else {
            all.insert(name, sha(&fs::read(&path).unwrap()));
        }
    }
}
fn compare_group(original: &str, blobs: &str, count: usize) {
    let original = expected::original(original);
    let blobs = expected::original(blobs);
    let rows = original["cases"].as_array().unwrap();
    assert_eq!(rows.len(), count);
    for row in rows {
        let temporary = tempfile::tempdir().unwrap();
        let output = temporary.path().join("output");
        let loop_mode = row["tool"] == "loop";
        let target = if loop_mode {
            output.clone()
        } else {
            output.join("records.json")
        };
        match row["case"].as_str().unwrap() {
            "output-directory" => fs::create_dir_all(if loop_mode {
                target.join("normal/source.json")
            } else {
                target.clone()
            })
            .unwrap(),
            "parent-file" => fs::write(&output, b"original-parent-marker").unwrap(),
            "native-directory" => fs::create_dir_all(output.join("normal/native.json")).unwrap(),
            "result-directory" => fs::create_dir_all(output.join("normal/result.json")).unwrap(),
            "second-native-directory" => {
                fs::create_dir_all(output.join("recovery/native.json")).unwrap()
            }
            _ => (),
        }
        let kind = match row["tool"].as_str().unwrap() {
            "numeric" => Kind::Numeric,
            "cache" => Kind::Cache,
            "packet" => Kind::Packet,
            "loop" => Kind::Loop,
            _ => unreachable!(),
        };
        let options = Options {
            root: Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.."))
                .canonicalize()
                .unwrap(),
            binary: "./reference".into(),
            output: target,
        };
        let mut peer = Peer {
            row,
            blobs: &blobs,
            calls: 0,
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let result: Result<()> = run(kind, &options, &mut peer, &mut stdout, &mut stderr);
        let label = format!("{}:{}", row["tool"], row["case"]);
        assert_eq!(
            result.is_err(),
            !row["error"].is_null(),
            "{label}: {:?}",
            result.err().map(|e| e.to_string())
        );
        assert_eq!(
            peer.calls,
            row["calls"].as_array().unwrap().len(),
            "{label}"
        );
        let stdout = String::from_utf8(stdout).unwrap();
        expected::compare(
            &JsonValue::text(row["stdout"].as_str().unwrap()),
            &JsonValue::text(&stdout),
            (0., 0.),
            true,
            &label,
        )
        .unwrap();
        let stderr = String::from_utf8(stderr)
            .unwrap()
            .replace(options.root.to_str().unwrap(), "@ROOT@");
        assert_eq!(stderr, row["stderr"].as_str().unwrap(), "{label}: stderr");
        let mut actual = BTreeMap::new();
        let mut directories = Vec::new();
        files(
            temporary.path(),
            temporary.path(),
            &mut actual,
            &mut directories,
        );
        directories.sort();
        let want = row["files"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(name, value)| {
                (
                    name.to_owned(),
                    value["sha256"].as_str().unwrap().to_owned(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        assert_eq!(
            actual.keys().collect::<Vec<_>>(),
            want.keys().collect::<Vec<_>>(),
            "{label}: artifact paths"
        );
        for (name, expected_hash) in &want {
            let actual_hash = &actual[name];
            if actual_hash == expected_hash {
                continue;
            }
            let expected_bytes = STANDARD
                .decode(blobs[expected_hash].as_str().unwrap())
                .unwrap();
            let actual_bytes = fs::read(temporary.path().join(name)).unwrap();
            // Source pose calculations use the original fixed numeric budget.
            // Every other token, error selector, order and format byte is exact.
            let reviewed = if row["tool"] == "numeric" && name == "output/records.json" {
                report::verify(&expected_bytes, &actual_bytes).map(|summary| {
                    assert!(
                        summary.source_float_differences > 0
                            || summary.derived_diagnostic_differences > 0
                    );
                })
            } else {
                Err("nonpose report bytes differ".to_owned())
            };
            if let Err(error) = reviewed {
                let first = expected_bytes
                    .iter()
                    .zip(&actual_bytes)
                    .position(|(a, b)| a != b)
                    .unwrap_or(expected_bytes.len().min(actual_bytes.len()));
                let expected_excerpt = String::from_utf8_lossy(
                    &expected_bytes
                        [first.saturating_sub(80)..(first + 120).min(expected_bytes.len())],
                );
                let actual_excerpt = String::from_utf8_lossy(
                    &actual_bytes[first.saturating_sub(80)..(first + 120).min(actual_bytes.len())],
                );
                eprintln!("{label} {name}: {error}; first byte difference {first}: original={expected_excerpt:?}; native={actual_excerpt:?}");
                if let Ok(directory) = std::env::var("LAGD_REVIEW_ARTIFACTS") {
                    let directory = std::path::PathBuf::from(directory).join(format!(
                        "{}-{}",
                        row["tool"].as_str().unwrap(),
                        row["case"].as_str().unwrap()
                    ));
                    fs::create_dir_all(&directory).unwrap();
                    fs::write(directory.join("original.json"), &expected_bytes).unwrap();
                    fs::write(directory.join("native.json"), &actual_bytes).unwrap();
                }
                panic!("{label}: {error}; original SHA {expected_hash}, native SHA {actual_hash}");
            }
        }
        assert_eq!(
            json!(directories),
            row["directories"],
            "{label}: directories"
        );
    }
}
#[test]
fn original_50_process_json_output_overwrite_and_error_boundaries() {
    compare_group("checker-original.json.gz", "checker-bytes.json.gz", 50)
}
#[test]
fn original_12_unicode_splitlines_and_extra_response_boundaries() {
    compare_group(
        "checker-lines-original.json.gz",
        "checker-lines-bytes.json.gz",
        12,
    )
}
#[test]
fn original_9_late_failures_error_membership_and_report_before_error() {
    compare_group(
        "checker-phases-original.json.gz",
        "checker-phases-bytes.json.gz",
        9,
    )
}

#[test]
fn new_report_guard_uses_original_pose_budget_and_rejects_corruption() {
    let original = r#"{"records":[{"case":"pose-test","input":{"kind":"pose"},"source":{"valid":true,"rotation":[[1.0]],"pose":[]},"native":null}],"failures":[]}"#;
    let within = original.replacen("[[1.0]]", "[[1.0000000000005]]", 1);
    assert_eq!(
        report::verify(original.as_bytes(), within.as_bytes())
            .unwrap()
            .source_float_differences,
        1
    );
    for corrupted in [
        original.replacen("[[1.0]]", "[[1.000000000004]]", 1),
        original.replacen("[[1.0]]", "[[null]]", 1),
        original.replacen("pose-test", "different-case", 1),
        original.replacen("\"native\":null", "\"native\":false", 1),
        original.replacen("\"valid\":true", "\"valid\":false", 1),
        original.replacen("\"pose\":[]", "\"pose\":[0]", 1),
        original.replacen("{\"records\":", "{ \"records\":", 1),
    ] {
        assert!(
            report::verify(original.as_bytes(), corrupted.as_bytes()).is_err(),
            "admitted {corrupted}"
        );
    }
    let other = original.replace("\"kind\":\"pose\"", "\"kind\":\"smooth\"");
    assert!(report::verify(
        other.as_bytes(),
        other
            .replacen("[[1.0]]", "[[1.0000000000005]]", 1)
            .as_bytes()
    )
    .is_err());
    let diagnostic = "('pose-test', {'valid': True, 'rotation': [[1.0]], 'pose': []}, None)";
    let original = original.replace(
        "\"failures\":[]",
        &format!(
            "\"failures\":[{}]",
            serde_json::to_string(diagnostic).unwrap()
        ),
    );
    let within = original.replace("[[1.0]]", "[[1.0000000000005]]");
    let summary = report::verify(original.as_bytes(), within.as_bytes()).unwrap();
    assert_eq!(summary.source_float_differences, 1);
    assert_eq!(summary.derived_diagnostic_differences, 1);
    for wrong in [
        diagnostic.replace("1.0", "1.1"),
        diagnostic.replace("pose-test", "different-case"),
        diagnostic.replace("None", "False"),
    ] {
        let corrupted = original.replace(
            &serde_json::to_string(diagnostic).unwrap(),
            &serde_json::to_string(&wrong).unwrap(),
        );
        assert!(
            report::verify(original.as_bytes(), corrupted.as_bytes()).is_err(),
            "admitted {wrong}"
        );
    }
}
