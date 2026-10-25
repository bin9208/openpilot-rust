use openpilot_dev_tools::{
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
    ui_scroll_check::{check, Kind},
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};
fn inflate(path: &Path) -> Vec<u8> {
    let mut out = vec![];
    flate2::read::GzDecoder::new(fs::File::open(path).unwrap())
        .read_to_end(&mut out)
        .unwrap();
    out
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
struct Peer<'a> {
    fixtures: &'a Path,
    row: &'a Value,
    calls: usize,
}
impl Runner for Peer<'_> {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        let case = self.calls;
        self.calls += 1;
        let kind = self.row["kind"].as_str().unwrap();
        assert_eq!(spec.argv, vec!["controlled-native"]);
        assert_eq!(
            spec.input.as_ref(),
            Some(&inflate(
                &self.fixtures.join(format!("{kind}-{case}-input.json.gz"))
            ))
        );
        assert_eq!(spec.timeout, Duration::MAX);
        assert_eq!(spec.stdout_limit, usize::MAX);
        assert_eq!(spec.stderr_limit, usize::MAX);
        assert!(spec.cwd.is_none());
        assert!(spec.effective_env().is_empty());
        if self.row["name"] == "spawn" {
            return Err("controlled missing candidate".into());
        }
        let target = case == self.row["target"].as_u64().unwrap() as usize;
        let reply = if target {
            self.fixtures
                .join("blobs")
                .join(format!("{}.gz", self.row["reply_sha256"].as_str().unwrap()))
        } else {
            self.fixtures.join(format!("{kind}-{case}-reply.json.gz"))
        };
        let stderr = if target {
            self.row["stderr_hex"]
                .as_str()
                .unwrap()
                .as_bytes()
                .chunks_exact(2)
                .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
                .collect()
        } else {
            vec![]
        };
        Ok(Captured {
            code: if target {
                self.row["status"].as_i64().unwrap() as i32
            } else {
                0
            },
            stdout: inflate(&reply),
            stderr,
        })
    }
}
#[test]
fn all_original_subprocess_cases_preserve_comparators_debug_prefixes_and_partial_files() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ui-scroll");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let rows: Value =
        serde_json::from_str(include_str!("fixtures/ui-scroll/failures.json")).unwrap();
    for row in rows.as_array().unwrap() {
        let kind = row["kind"].as_str().unwrap();
        let name = row["name"].as_str().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("out");
        if name.starts_with("stale-") {
            fs::create_dir(&output).unwrap();
            for case in 0..3 {
                for lane in ["input", "source", "native"] {
                    fs::write(
                        output.join(format!("{kind}-{case}-{lane}.json")),
                        format!("old-{case}{lane}"),
                    )
                    .unwrap();
                }
            }
            fs::write(output.join("untouched"), b"keep").unwrap();
        }
        let mut peer = Peer {
            fixtures: &fixtures,
            row,
            calls: 0,
        };
        let mut printed = vec![];
        let result = check(
            &root,
            Path::new("controlled-native"),
            &output,
            if kind == "scroll" {
                Kind::Scroll
            } else {
                Kind::Scroller
            },
            row["debug"].as_bool().unwrap(),
            &mut peer,
            &mut printed,
        );
        assert_eq!(
            result.is_ok(),
            row["exit"] == 0,
            "{kind}/{name}/debug{}: {result:?}",
            row["debug"]
        );
        let expected_calls =
            row["executed_cases"].as_array().unwrap().len() + usize::from(name == "spawn");
        assert_eq!(peer.calls, expected_calls, "{kind}/{name}");
        assert_eq!(
            sha(&printed),
            row["stdout_sha256"].as_str().unwrap(),
            "{kind}/{name}/debug{}",
            row["debug"]
        );
        let mut actual = serde_json::Map::new();
        for entry in fs::read_dir(&output).unwrap() {
            let p = entry.unwrap().path();
            let bytes = fs::read(&p).unwrap();
            actual.insert(
                p.file_name().unwrap().to_str().unwrap().into(),
                serde_json::json!({"bytes":bytes.len(),"sha256":sha(&bytes)}),
            );
        }
        assert_eq!(
            Value::Object(actual),
            row["files"],
            "{kind}/{name}/debug{}",
            row["debug"]
        );
    }
}

#[test]
fn original_cli_help_and_errors_have_no_candidate_side_effects() {
    let rows: Value = serde_json::from_str(include_str!("fixtures/ui-scroll/cli.json")).unwrap();
    for row in rows.as_array().unwrap() {
        let binary = if row["kind"] == "scroll" {
            env!("CARGO_BIN_EXE_check-ui-scroll")
        } else {
            env!("CARGO_BIN_EXE_check-ui-scroller")
        };
        let temp = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(binary)
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
            output.status.code(),
            row["exit"].as_i64().map(|v| v as i32),
            "{row}"
        );
        assert_eq!(
            !output.stdout.is_empty(),
            row["help"].as_bool().unwrap(),
            "{row}"
        );
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
    }
}
struct PhaseWriter {
    output: PathBuf,
    kind: &'static str,
    scene: usize,
    bytes: Vec<u8>,
    fail: bool,
}
impl std::io::Write for PhaseWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.scene < 3 {
            assert!(!self
                .output
                .join(format!("{}-{}-input.json", self.kind, self.scene))
                .exists());
            assert!(!self
                .output
                .join(format!("{}-{}-source.json", self.kind, self.scene))
                .exists());
            if self.scene > 0 {
                assert!(self
                    .output
                    .join(format!("{}-{}-native.json", self.kind, self.scene - 1))
                    .exists());
            }
            self.scene += 1;
        }
        if self.fail {
            return Err(std::io::Error::other("controlled output failure"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
#[test]
fn debug_writer_precedes_scene_artifacts_and_write_failure_stops_child() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ui-scroll");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let rows: Value =
        serde_json::from_str(include_str!("fixtures/ui-scroll/failures.json")).unwrap();
    for (name, kind) in [("scroll", Kind::Scroll), ("scroller", Kind::Scroller)] {
        let row = rows
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["kind"] == name && r["name"] == "success" && r["debug"] == true)
            .unwrap();
        for fail in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let output = temp.path().join("out");
            let mut writer = PhaseWriter {
                output: output.clone(),
                kind: name,
                scene: 0,
                bytes: vec![],
                fail,
            };
            let mut peer = Peer {
                fixtures: &fixtures,
                row,
                calls: 0,
            };
            let result = check(
                &root,
                Path::new("controlled-native"),
                &output,
                kind,
                true,
                &mut peer,
                &mut writer,
            );
            assert_eq!(result.is_err(), fail);
            assert_eq!(peer.calls, if fail { 0 } else { 3 });
            if fail {
                assert_eq!(fs::read_dir(output).unwrap().count(), 0);
            } else {
                assert_eq!(writer.scene, 3);
                assert_eq!(sha(&writer.bytes), row["stdout_sha256"].as_str().unwrap());
            }
        }
    }
}
#[test]
fn source_and_dependency_drift_fail_before_candidate() {
    for (name, kind) in [("scroll", Kind::Scroll), ("scroller", Kind::Scroller)] {
        let root = tempfile::tempdir().unwrap();
        let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let pins: Value =
            serde_json::from_str(include_str!("fixtures/ui-scroll/source-pins.json")).unwrap();
        for path in pins[name].as_object().unwrap().keys() {
            let dest = root.path().join(path);
            fs::create_dir_all(dest.parent().unwrap()).unwrap();
            fs::copy(source_root.join(path), dest).unwrap();
        }
        fs::copy(source_root.join("uv.lock"), root.path().join("uv.lock")).unwrap();
        for dependency in [false, true] {
            if dependency && name == "scroll" {
                continue;
            }
            if dependency {
                fs::write(root.path().join("uv.lock"), "package=[]\n").unwrap();
            } else {
                let path = pins[name].as_object().unwrap().keys().next().unwrap();
                fs::write(root.path().join(path), b"changed").unwrap();
            }
            let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ui-scroll");
            let row = serde_json::json!({});
            let mut peer = Peer {
                fixtures: &fixtures,
                row: &row,
                calls: 0,
            };
            assert!(check(
                root.path(),
                Path::new("controlled-native"),
                &root.path().join("out"),
                kind,
                false,
                &mut peer,
                &mut vec![]
            )
            .is_err());
            assert_eq!(peer.calls, 0);
            // Restore sources so the second failure independently tests the lock.
            for path in pins[name].as_object().unwrap().keys() {
                fs::copy(source_root.join(path), root.path().join(path)).unwrap();
            }
            if dependency && name == "scroll" {
                break;
            }
        }
    }
}
