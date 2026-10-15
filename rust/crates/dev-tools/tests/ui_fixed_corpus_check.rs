use openpilot_dev_tools::{
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
    ui_fixed_corpus_check::{check, Kind},
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::Path};
fn inflate(path: &Path) -> Vec<u8> {
    let mut out = vec![];
    flate2::read::GzDecoder::new(fs::File::open(path).unwrap())
        .read_to_end(&mut out)
        .unwrap();
    out
}
struct Peer {
    reply: Vec<u8>,
    stderr: Vec<u8>,
    status: i32,
    spawn: bool,
    calls: usize,
    input: Vec<u8>,
}
impl Runner for Peer {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        self.calls += 1;
        assert_eq!(spec.input.as_ref(), Some(&self.input));
        assert_eq!(
            spec.effective_argv(),
            vec![std::ffi::OsString::from("candidate")]
        );
        assert!(spec.cwd.is_none());
        if self.spawn {
            return Err("missing candidate".into());
        }
        Ok(Captured {
            code: self.status,
            stdout: self.reply.clone(),
            stderr: self.stderr.clone(),
        })
    }
}
#[test]
fn independently_captured_original_success_and_failure_artifacts() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ui-fixed-corpus");
    let rows: Value =
        serde_json::from_slice(&fs::read(fixtures.join("failures.json")).unwrap()).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for row in rows.as_array().unwrap() {
        let kind = row["kind"].as_str().unwrap();
        let name = row["case"].as_str().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("output");
        if name.starts_with("stale-") {
            fs::create_dir(&output).unwrap();
            fs::write(output.join("result.json"), b"old-result").unwrap();
            fs::write(output.join("untouched"), b"keep").unwrap();
        }
        let stderr = row["reply_stderr_hex"]
            .as_str()
            .unwrap()
            .as_bytes()
            .chunks_exact(2)
            .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
            .collect();
        let mut peer = Peer {
            reply: inflate(
                &fixtures
                    .join("replies")
                    .join(format!("{}.gz", row["reply_sha256"].as_str().unwrap())),
            ),
            stderr,
            status: row["status"].as_i64().unwrap() as i32,
            spawn: name == "spawn",
            calls: 0,
            input: inflate(&fixtures.join(format!("{kind}-input.json.gz"))),
        };
        let result = check(
            &root,
            Path::new("candidate"),
            &output,
            if kind == "qr" { Kind::Qr } else { Kind::Torque },
            &mut peer,
        );
        assert_eq!(
            result.is_ok(),
            row["exit"] == 0,
            "{kind}/{name}: {result:?}"
        );
        assert_eq!(peer.calls, 1);
        let mut actual = serde_json::Map::new();
        for entry in fs::read_dir(&output).unwrap() {
            let path = entry.unwrap().path();
            let bytes = fs::read(&path).unwrap();
            actual.insert(path.file_name().unwrap().to_str().unwrap().into(),serde_json::json!({"bytes":bytes.len(),"sha256":format!("{:x}",Sha256::digest(&bytes))}));
        }
        assert_eq!(Value::Object(actual), row["files"], "{kind}/{name}");
        if let Ok(message) = result {
            assert_eq!(message, row["stdout"].as_str().unwrap());
        }
    }
}
#[test]
fn original_whole_script_parse_and_serializer_depth_boundaries() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ui-fixed-corpus");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let source: Value =
        serde_json::from_slice(&inflate(&fixtures.join("torque-source.json.gz"))).unwrap();
    for (depth, serializer) in [(9997, false), (9998, false), (993, true), (994, true)] {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("out");
        let mut reply = format!("{}0{}", "[".repeat(depth), "]".repeat(depth));
        if serializer {
            let rest = serde_json::to_string(&source.as_array().unwrap()[1..]).unwrap();
            reply = format!("[{reply},{}", &rest[1..]);
        }
        let mut peer = Peer {
            reply: reply.into_bytes(),
            stderr: vec![],
            status: 0,
            spawn: false,
            calls: 0,
            input: inflate(&fixtures.join("torque-input.json.gz")),
        };
        assert!(check(
            &root,
            Path::new("candidate"),
            &output,
            Kind::Torque,
            &mut peer
        )
        .is_err());
        assert_eq!(output.join("source.json").exists(), depth != 9998);
        assert_eq!(
            output.join("result.json").exists(),
            serializer && depth == 993
        );
    }
}
#[test]
fn changed_source_and_dependency_pins_fail_before_candidate_or_artifacts() {
    let original = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for kind in [Kind::Qr, Kind::Torque] {
        let temp = tempfile::tempdir().unwrap();
        fs::copy(original.join("uv.lock"), temp.path().join("uv.lock")).unwrap();
        let file = temp.path().join("uv.lock");
        let text = fs::read_to_string(&file).unwrap();
        fs::write(
            file,
            text.replace(
                if matches!(kind, Kind::Qr) {
                    "version = \"8.2\""
                } else {
                    "version = \"2.5.3\""
                },
                "version = \"99\"",
            ),
        )
        .unwrap();
        let mut peer = Peer {
            reply: vec![],
            stderr: vec![],
            status: 0,
            spawn: false,
            calls: 0,
            input: vec![],
        };
        let output = temp.path().join("out");
        assert!(check(
            temp.path(),
            Path::new("candidate"),
            &output,
            kind,
            &mut peer
        )
        .is_err());
        assert_eq!(peer.calls, 0);
        assert!(!output.exists());
    }
}

#[test]
fn changed_torque_runtime_source_is_rejected_without_output() {
    let original = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let temp = tempfile::tempdir().unwrap();
    fs::copy(original.join("uv.lock"), temp.path().join("uv.lock")).unwrap();
    let source = temp
        .path()
        .join("openpilot/selfdrive/ui/mici/onroad/torque_bar.py");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(source, b"changed original source").unwrap();
    let mut peer = Peer {
        reply: vec![],
        stderr: vec![],
        status: 0,
        spawn: false,
        calls: 0,
        input: vec![],
    };
    let output = temp.path().join("out");
    assert!(check(
        temp.path(),
        Path::new("candidate"),
        &output,
        Kind::Torque,
        &mut peer
    )
    .is_err());
    assert_eq!(peer.calls, 0);
    assert!(!output.exists());
}
#[test]
fn original_cli_help_and_syntax_errors_have_no_output_side_effects() {
    let rows: Value =
        serde_json::from_str(include_str!("fixtures/ui-fixed-corpus/cli.json")).unwrap();
    for row in rows.as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let binary = if row["kind"] == "qr" {
            env!("CARGO_BIN_EXE_check-ui-qr")
        } else {
            env!("CARGO_BIN_EXE_check-ui-torque-geometry")
        };
        let args = row["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>();
        let status = std::process::Command::new(binary)
            .args(args)
            .current_dir(temp.path())
            .output()
            .unwrap();
        assert_eq!(
            status.status.code(),
            Some(row["exit"].as_i64().unwrap() as i32),
            "{row}"
        );
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
    }
}
