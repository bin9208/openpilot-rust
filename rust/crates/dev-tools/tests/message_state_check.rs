use openpilot_dev_tools::{
    message_state_check::check,
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
fn reconstruct(fixtures: &Path, p: &Value) -> Vec<u8> {
    let base = inflate(&fixtures.join("expected.gz"));
    let insert = inflate(
        &fixtures
            .join("patches")
            .join(format!("{}.gz", p["insert_sha256"].as_str().unwrap())),
    );
    assert_eq!(sha(&insert), p["insert_sha256"]);
    let mut b = base[..p["prefix"].as_u64().unwrap() as usize].to_vec();
    b.extend(insert);
    let suffix = p["suffix"].as_u64().unwrap() as usize;
    if suffix > 0 {
        b.extend(&base[base.len() - suffix..]);
    }
    assert_eq!(sha(&b), p["actual_sha256"]);
    b
}
struct Peer {
    fixtures: PathBuf,
    row: Value,
    output: PathBuf,
    calls: usize,
}
impl Runner for Peer {
    fn run(&mut self, s: &CommandSpec) -> Result<Captured> {
        self.calls += 1;
        let args = s.effective_argv();
        assert_eq!(args.len(), 3);
        assert!(Path::new(&args[0]).is_absolute());
        assert!(Path::new(&args[1]).is_absolute());
        assert_eq!(Path::new(&args[1]).file_name().unwrap(), "request.jsonl");
        assert_eq!(Path::new(&args[2]).file_name().unwrap(), "actual.jsonl");
        assert_eq!(s.timeout, Duration::MAX);
        assert!(s.inherit_output);
        assert!(s.inherit_stdin);
        assert!(s.input.is_none());
        assert!(s.effective_env().is_empty());
        let manifest: Value =
            serde_json::from_slice(&fs::read(self.fixtures.join("manifest.json"))?)?;
        assert_eq!(
            sha(&fs::read(&args[1])?),
            manifest["corpora"]["request.gz"]["sha256"]
        );
        assert_eq!(
            sha(&fs::read(self.output.join("expected.jsonl"))?),
            manifest["corpora"]["expected.gz"]["sha256"]
        );
        if self.row["mode"] == "spawn-error" {
            return Err("controlled spawn failure".into());
        }
        if self.row["expected_changed"] == true {
            let path = self.output.join("expected.jsonl");
            if self.row["expected_patch"].is_null() {
                fs::remove_file(path)?;
            } else {
                fs::write(
                    path,
                    reconstruct(&self.fixtures, &self.row["expected_patch"]),
                )?;
            }
        }
        if self.row["actual_patch"].is_object() {
            fs::write(
                &args[2],
                reconstruct(&self.fixtures, &self.row["actual_patch"]),
            )?;
        }
        Ok(Captured {
            code: if self.row["mode"] == "exit" { 7 } else { 0 },
            stdout: vec![],
            stderr: vec![],
        })
    }
}
fn captured_cases(expected_effects: bool, count_effects: bool) {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/message-state");
    let cases: Value =
        serde_json::from_slice(&fs::read(fixtures.join("original-cases.json")).unwrap()).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for row in cases["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| (r["expected_changed"] == true) == expected_effects)
        .filter(|r| {
            matches!(
                r["mode"].as_str(),
                Some("expected-short-match" | "expected-empty-match" | "expected-appended-match")
            ) == count_effects
        })
    {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("out");
        if row["mode"] == "output-exists" {
            fs::create_dir(&output).unwrap();
        }
        let mut peer = Peer {
            fixtures: fixtures.clone(),
            row: row.clone(),
            output: output.clone(),
            calls: 0,
        };
        let result = check(&root, Path::new("./controlled-native"), &output, &mut peer);
        if row["mode"] == "expected-invalid-actual-missing" {
            assert_eq!(
                result
                    .as_ref()
                    .unwrap_err()
                    .downcast_ref::<std::io::Error>()
                    .map(std::io::Error::kind),
                Some(std::io::ErrorKind::NotFound)
            );
        }
        assert_eq!(
            result.is_ok(),
            row["exit"] == 0,
            "{} {result:?}",
            row["mode"]
        );
        assert_eq!(
            peer.calls,
            if row["mode"] == "output-exists" { 0 } else { 1 }
        );
        if let Ok(stdout) = result {
            assert_eq!(stdout, row["printed_report"]);
        }
        let files = fs::read_dir(&output)
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
        assert_eq!(files, expected, "{}", row["mode"]);
    }
}

#[test]
fn original_full_cli_maximum_error_numeric_admission_and_artifact_effects() {
    captured_cases(false, false);
}
#[test]
fn original_rereads_child_modified_expected_file() {
    captured_cases(true, false);
}
#[test]
fn original_report_count_follows_post_child_paired_files() {
    captured_cases(true, true);
}
struct Never;
impl Runner for Never {
    fn run(&mut self, _: &CommandSpec) -> Result<Captured> {
        panic!("candidate must not run after source/schema/dependency drift")
    }
}
#[test]
fn live_source_schema_and_dependency_guards_remain_required() {
    let f = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/message-state");
    let m: Value = serde_json::from_slice(&fs::read(f.join("manifest.json")).unwrap()).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for changed in [
        "rust/tools/check_message_state.py",
        "openpilot/cereal/log.capnp",
        "uv.lock",
    ] {
        let temp = tempfile::tempdir().unwrap();
        for path in m["sources"].as_object().unwrap().keys() {
            let dest = temp.path().join(path);
            fs::create_dir_all(dest.parent().unwrap()).unwrap();
            fs::copy(root.join(path), dest).unwrap();
        }
        fs::copy(root.join("uv.lock"), temp.path().join("uv.lock")).unwrap();
        fs::write(
            temp.path().join(changed),
            if changed == "uv.lock" {
                "package=[]"
            } else {
                "drift"
            },
        )
        .unwrap();
        let output = temp.path().join("out");
        let result = check(temp.path(), Path::new("never"), &output, &mut Never);
        assert!(result.is_err(), "{changed}");
        assert!(!output.exists());
    }
}
