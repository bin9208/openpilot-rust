use openpilot_dev_tools::{
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
    ui_headless_check::{check, Kind},
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
struct Peer {
    reply: Vec<u8>,
    row: Value,
    calls: usize,
}
impl Runner for Peer {
    fn run(&mut self, s: &CommandSpec) -> Result<Captured> {
        self.calls += 1;
        assert_eq!(
            s.effective_argv(),
            vec![std::ffi::OsString::from("controlled-native")]
        );
        assert_eq!(sha(s.input.as_ref().unwrap()), self.row["input_sha256"]);
        assert_eq!(s.timeout, Duration::MAX);
        assert_eq!(s.stdout_limit, usize::MAX);
        assert_eq!(s.stderr_limit, usize::MAX);
        assert!(s.cwd.is_none());
        assert!(s.effective_env().is_empty());
        if self.row["spawn_error"] == true {
            return Err("controlled candidate spawn failure".into());
        }
        let stderr = self.row["stderr_hex"]
            .as_str()
            .unwrap()
            .as_bytes()
            .chunks_exact(2)
            .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
            .collect();
        Ok(Captured {
            code: self.row["status"].as_i64().unwrap() as i32,
            stdout: self.reply.clone(),
            stderr,
        })
    }
}
#[test]
fn independent_original_comparators_and_artifact_effects() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ui-headless");
    let report: Value =
        serde_json::from_slice(&fs::read(fixtures.join("original-cases.json")).unwrap()).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for row in report["cases"].as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("out");
        let mut peer = Peer {
            reply: inflate(
                &fixtures
                    .join("replies")
                    .join(format!("{}.gz", row["reply_sha256"].as_str().unwrap())),
            ),
            row: row.clone(),
            calls: 0,
        };
        let kind = if row["family"] == "widgets" {
            Kind::Widgets
        } else {
            Kind::Navigation
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
            "{} {}: {result:?}",
            row["family"],
            row["name"]
        );
        assert_eq!(peer.calls, 1);
        if let Ok(printed) = result {
            assert_eq!(printed, row["stdout"])
        }
        let actual = fs::read_dir(&output)
            .unwrap()
            .map(|e| {
                let e = e.unwrap();
                (
                    e.file_name().into_string().unwrap(),
                    sha(&fs::read(e.path()).unwrap()),
                )
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        let expected = row["files"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned()))
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(actual, expected, "{} {}", row["family"], row["name"]);
    }
}
struct Never;
impl Runner for Never {
    fn run(&mut self, _: &CommandSpec) -> Result<Captured> {
        panic!("candidate must not run after source/dependency drift")
    }
}
#[test]
fn source_and_navigation_dependency_drift_fail_before_candidate() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ui-headless");
    let manifest: Value =
        serde_json::from_slice(&fs::read(fixtures.join("original-cases.json")).unwrap()).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for kind in [Kind::Widgets, Kind::Navigation] {
        let temp = tempfile::tempdir().unwrap();
        let key = if matches!(kind, Kind::Widgets) {
            "widgets"
        } else {
            "navigation"
        };
        for (path, _) in manifest["sources"][key].as_object().unwrap() {
            let dest = temp.path().join(path);
            fs::create_dir_all(dest.parent().unwrap()).unwrap();
            fs::copy(root.join(path), dest).unwrap();
        }
        fs::copy(root.join("uv.lock"), temp.path().join("uv.lock")).unwrap();
        fs::write(
            temp.path().join("openpilot/system/ui/widgets/__init__.py"),
            "source drift",
        )
        .unwrap();
        assert!(check(
            temp.path(),
            Path::new("never"),
            &temp.path().join("out"),
            kind,
            &mut Never
        )
        .unwrap_err()
        .to_string()
        .contains("source changed"));
        assert!(!temp.path().join("out").exists());
    }
    let temp = tempfile::tempdir().unwrap();
    for (path, _) in manifest["sources"]["navigation"].as_object().unwrap() {
        let dest = temp.path().join(path);
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::copy(root.join(path), dest).unwrap();
    }
    fs::write(temp.path().join("uv.lock"), "package = []\n").unwrap();
    assert!(check(
        temp.path(),
        Path::new("never"),
        &temp.path().join("out"),
        Kind::Navigation,
        &mut Never
    )
    .unwrap_err()
    .to_string()
    .contains("numpy source changed"));
}
