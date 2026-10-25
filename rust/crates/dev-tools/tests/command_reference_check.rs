use openpilot_dev_tools::{
    command_reference_check::{check, Kind},
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
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn blob(root: &Path, hash: &str) -> Vec<u8> {
    let mut out = vec![];
    flate2::read::GzDecoder::new(
        fs::File::open(root.join("blobs").join(format!("{hash}.gz"))).unwrap(),
    )
    .read_to_end(&mut out)
    .unwrap();
    out
}
fn replace(bytes: Vec<u8>, old: &[u8], new: &[u8]) -> Vec<u8> {
    let mut out = vec![];
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i..].starts_with(old) {
            out.extend_from_slice(new);
            i += old.len();
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}
struct Peer<'a> {
    row: &'a Value,
    fixtures: PathBuf,
    root: PathBuf,
    output: PathBuf,
    binary: PathBuf,
    calls: usize,
}
impl Runner for Peer<'_> {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        self.calls += 1;
        assert_eq!(spec.timeout, Duration::MAX);
        assert!(spec.inherit_output);
        assert!(spec.inherit_stdin);
        assert!(spec.cwd.is_none());
        assert!(spec.effective_env().is_empty());
        let kind = self.row["kind"].as_str().unwrap();
        if self.row["mode"] == "spawn" {
            return Err("controlled missing command probe".into());
        }
        let journal = self.output.join(if kind == "reference" {
            "rust"
        } else {
            "native"
        });
        assert_eq!(
            spec.argv,
            vec![
                self.binary.to_str().unwrap(),
                self.output.join("request.jsonl").to_str().unwrap(),
                self.output.join("actual.jsonl").to_str().unwrap(),
                journal.to_str().unwrap()
            ]
        );
        assert!(!journal.exists());
        fs::create_dir(&journal)?;
        fs::write(journal.join("owned"), b"candidate")?;
        let mode = self.row["mode"].as_str().unwrap();
        let actual = self.output.join("actual.jsonl");
        if !matches!(mode, "nonzero" | "missing-actual") {
            if mode == "actual-directory" {
                fs::create_dir(&actual)?;
            } else {
                fs::write(
                    &actual,
                    blob(
                        &self.fixtures,
                        self.row["candidate_reply_blob"].as_str().unwrap(),
                    ),
                )?;
            }
        }
        match mode {
            "request-directory" => {
                fs::remove_file(self.output.join("request.jsonl"))?;
                fs::create_dir(self.output.join("request.jsonl"))?;
            }
            "reference-file-edit" => fs::write(self.output.join("reference.json"), b"changed")?,
            "root-file-edit" => fs::write(self.output.join("original/lane.json"), b"changed")?,
            "source-mutate" => fs::write(
                self.root
                    .join("openpilot/selfdrive/carrot/bluetooth/model.py"),
                b"changed",
            )?,
            "source-delete" => fs::remove_file(
                self.root
                    .join("openpilot/selfdrive/carrot/bluetooth/model.py"),
            )?,
            "source-unreadable-directory" => {
                let p = self
                    .root
                    .join("openpilot/selfdrive/carrot/bluetooth/model.py");
                fs::remove_file(&p)?;
                fs::create_dir(p)?;
            }
            "binary-mutate" => fs::write(&self.binary, b"changed")?,
            "binary-delete" => fs::remove_file(&self.binary)?,
            _ => {}
        }
        Ok(Captured {
            code: self.row["candidate_exit"].as_i64().unwrap() as i32,
            stdout: vec![],
            stderr: vec![],
        })
    }
}
fn files(
    path: &Path,
    base: &Path,
    out: &mut std::collections::BTreeMap<String, Vec<u8>>,
    dirs: &mut Vec<String>,
) {
    for entry in fs::read_dir(path).unwrap() {
        let p = entry.unwrap().path();
        if p.is_dir() {
            dirs.push(p.strip_prefix(base).unwrap().to_str().unwrap().into());
            files(&p, base, out, dirs);
        } else {
            out.insert(
                p.strip_prefix(base).unwrap().to_str().unwrap().into(),
                fs::read(p).unwrap(),
            );
        }
    }
}
#[test]
fn complete_original_cases_preserve_journal_contents_cached_oracle_comparisons_and_receipt_phases()
{
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/command-reference");
    let rows: Value = serde_json::from_str(include_str!(
        "fixtures/command-reference/original-cases.json"
    ))
    .unwrap();
    for row in rows.as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        let model = root.join("openpilot/selfdrive/carrot/bluetooth/model.py");
        fs::create_dir_all(model.parent().unwrap()).unwrap();
        fs::copy(
            source_root.join("openpilot/selfdrive/carrot/bluetooth/model.py"),
            &model,
        )
        .unwrap();
        for name in [
            "openpilot/__init__.py",
            "openpilot/selfdrive/__init__.py",
            "openpilot/selfdrive/carrot/bluetooth/__init__.py",
        ] {
            fs::copy(source_root.join(name), root.join(name)).unwrap();
        }
        let output = temp.path().join("out");
        let binary = temp.path().join("candidate");
        fs::write(&binary, b"controlled-command-probe\n").unwrap();
        let mode = row["mode"].as_str().unwrap();
        let kind = if row["kind"] == "reference" {
            Kind::Reference
        } else {
            Kind::JsonBoundary
        };
        if mode == "existing-output" {
            fs::create_dir(&output).unwrap();
            fs::write(output.join("untouched"), b"keep").unwrap();
        } else if mode == "output-file" {
            fs::write(&output, b"file").unwrap();
        }
        let mut peer = Peer {
            row,
            fixtures: fixtures.clone(),
            root: root.clone(),
            output: output.clone(),
            binary: binary.clone(),
            calls: 0,
        };
        let mut printed = vec![];
        let result = check(&root, &binary, &output, kind, &mut peer, &mut printed);
        assert_eq!(
            result.is_ok(),
            row["exit"] == 0,
            "{}/{mode}: {result:?}",
            row["kind"]
        );
        assert_eq!(
            peer.calls,
            usize::from(row["called"].as_bool().unwrap() || mode == "spawn")
        );
        let normalize = |b: Vec<u8>| {
            let old = row["model_path"].as_str().unwrap().as_bytes();
            let b = replace(b, old, model.to_str().unwrap().as_bytes());
            if let Some(hash) = row["binary_sha256_after"].as_str() {
                if let Ok(bytes) = fs::read(&binary) {
                    replace(b, hash.as_bytes(), sha(&bytes).as_bytes())
                } else {
                    b
                }
            } else {
                b
            }
        };
        let old = blob(&fixtures, row["stdout_blob"].as_str().unwrap());
        let old = if row["called"] == true {
            old.strip_prefix(b"raw-\xff\n").unwrap().to_vec()
        } else {
            old
        };
        assert_eq!(printed, normalize(old), "{}/{mode} printed", row["kind"]);
        if output.is_dir() {
            let mut actual = std::collections::BTreeMap::new();
            let mut dirs = vec![];
            files(&output, &output, &mut actual, &mut dirs);
            dirs.sort();
            assert_eq!(
                dirs,
                row["directories"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().to_owned())
                    .collect::<Vec<_>>(),
                "{mode} directories"
            );
            assert_eq!(
                actual.len(),
                row["files"].as_object().unwrap().len(),
                "{mode}"
            );
            for (name, entry) in row["files"].as_object().unwrap() {
                let expected = normalize(blob(&fixtures, entry["blob"].as_str().unwrap()));
                assert_eq!(
                    actual.get(name).unwrap(),
                    &expected,
                    "{}/{mode}/{name}",
                    row["kind"]
                );
            }
        }
    }
}
#[test]
fn original_argparse_help_and_errors_leave_output_uncreated() {
    let rows: Value =
        serde_json::from_str(include_str!("fixtures/command-reference/original-cli.json")).unwrap();
    for row in rows.as_array().unwrap() {
        let binary = if row["kind"] == "reference" {
            env!("CARGO_BIN_EXE_check-command-reference")
        } else {
            env!("CARGO_BIN_EXE_check-command-json-boundary")
        };
        let temp = tempfile::tempdir().unwrap();
        let result = std::process::Command::new(binary)
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
            result.status.code(),
            row["exit"].as_i64().map(|v| v as i32),
            "{row}"
        );
        assert_eq!(
            !result.stdout.is_empty(),
            row["help"].as_bool().unwrap(),
            "{row}"
        );
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
    }
}
#[test]
fn original_policy_package_and_namespace_drift_fail_before_source_journals_or_child() {
    struct Never(usize);
    impl Runner for Never {
        fn run(&mut self, _: &CommandSpec) -> Result<Captured> {
            self.0 += 1;
            Err("unexpected child".into())
        }
    }
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let manifest: Value =
        serde_json::from_str(include_str!("fixtures/command-reference/manifest.json")).unwrap();
    for changed in [
        "openpilot/selfdrive/carrot/bluetooth/model.py",
        "openpilot/selfdrive/carrot/bluetooth/__init__.py",
        "openpilot/selfdrive/carrot/__init__.py",
    ] {
        for kind in [Kind::Reference, Kind::JsonBoundary] {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("root");
            for path in manifest["sources"].as_object().unwrap().keys() {
                let destination = root.join(path);
                fs::create_dir_all(destination.parent().unwrap()).unwrap();
                fs::copy(source_root.join(path), destination).unwrap();
            }
            fs::write(root.join(changed), b"changed").unwrap();
            let output = temp.path().join("out");
            let mut peer = Never(0);
            assert!(check(
                &root,
                Path::new("candidate"),
                &output,
                kind,
                &mut peer,
                &mut vec![]
            )
            .is_err());
            assert_eq!(peer.0, 0);
            assert!(!output.exists());
        }
    }
}
