use openpilot_dev_tools::{
    control_tools_policy_check,
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use serde_json::Value;
use std::{collections::BTreeMap, fs};
fn bytes(value: &Value) -> Vec<u8> {
    value
        .as_str()
        .unwrap()
        .as_bytes()
        .chunks_exact(2)
        .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
        .collect()
}
struct Peer<'a> {
    row: &'a Value,
    calls: usize,
    binary: String,
}
impl Runner for Peer<'_> {
    fn run(&mut self, command: &CommandSpec) -> Result<Captured> {
        self.calls += 1;
        assert_eq!(command.argv, std::slice::from_ref(&self.binary));
        assert_eq!(
            command.input.as_deref(),
            Some(
                self.row
                    .get("request")
                    .map(bytes)
                    .unwrap_or_else(|| b"{\"cases\":[]}\n".to_vec())
            )
            .as_deref()
        );
        if self.row["spawn"] == true {
            return Err(std::io::Error::from(std::io::ErrorKind::NotFound).into());
        }
        Ok(Captured {
            code: self.row["code"].as_i64().unwrap_or(0) as i32,
            stdout: bytes(&self.row["stdout"]),
            stderr: self.row.get("stderr").map(bytes).unwrap_or_default(),
        })
    }
}
#[test]
fn original_comparator_receipts_rejections_and_partial_outputs_match() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/control-tools-policy/original.json")).unwrap();
    for row in fixture["cases"].as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let source = root.join("source");
        let output = root.join("output");
        let binary = root.join("probe");
        fs::create_dir(&source).unwrap();
        fs::write(&binary, b"owned-probe\n").unwrap();
        fs::write(
            source.join("input.json"),
            row.get("request")
                .map(bytes)
                .unwrap_or_else(|| b"{\"cases\":[]}\n".to_vec()),
        )
        .unwrap();
        fs::write(source.join("expected.json"), bytes(&row["expected"])).unwrap();
        if row["exists"] == true {
            fs::create_dir(&output).unwrap();
        }
        let mut peer = Peer {
            row,
            calls: 0,
            binary: binary.to_string_lossy().into_owned(),
        };
        let mut printed = vec![];
        let result =
            control_tools_policy_check::check(&binary, &source, &output, &mut peer, &mut printed);
        assert_eq!(
            result.is_ok(),
            row["error"].is_null(),
            "{}: {result:?}",
            row["name"]
        );
        assert_eq!(
            peer.calls,
            row["calls"].as_u64().unwrap() as usize,
            "{}",
            row["name"]
        );
        let mut actual = BTreeMap::new();
        if output.is_dir() {
            for entry in fs::read_dir(&output).unwrap() {
                let p = entry.unwrap().path();
                let mut data = fs::read(&p).unwrap();
                if p.file_name().unwrap() == "receipt.json" {
                    data = String::from_utf8(data)
                        .unwrap()
                        .replace(&peer.binary, "BINARY")
                        .into_bytes();
                }
                actual.insert(p.file_name().unwrap().to_string_lossy().into_owned(), data);
            }
        }
        let expected = row["files"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), bytes(v)))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(actual, expected, "{}", row["name"]);
        assert_eq!(
            String::from_utf8(printed)
                .unwrap()
                .replace(&peer.binary, "BINARY"),
            row["printed"].as_str().unwrap(),
            "{}",
            row["name"]
        );
    }
}

#[test]
fn actual_cli_uses_raw_stdin_and_preserves_existing_output_on_argument_errors() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let source = root.join("source");
    fs::create_dir(&source).unwrap();
    let data = b"[[{\"value\": 9007199254740993, \"raw\": \"\\ud800\"}]]\r\n";
    fs::write(source.join("input.json"), data).unwrap();
    fs::write(source.join("expected.json"), data).unwrap();
    let probe = root.join("probe");
    fs::write(&probe, b"#!/bin/sh\ncat\nprintf 'raw\\377\\r' >&2\n").unwrap();
    fs::set_permissions(&probe, fs::Permissions::from_mode(0o700)).unwrap();
    let output = root.join("output");
    let binary = env!("CARGO_BIN_EXE_check-control-tools-policy");
    let result = std::process::Command::new(binary)
        .args([
            format!("--bin={}", probe.display()),
            "--sou".into(),
            source.display().to_string(),
            "--out".into(),
            output.display().to_string(),
        ])
        .output()
        .unwrap();
    assert!(result.status.success(), "{:?}", result);
    assert_eq!(fs::read(output.join("stdout.json")).unwrap(), data);
    assert_eq!(fs::read(output.join("stderr.log")).unwrap(), b"raw\xff\r");
    let receipt: Value =
        serde_json::from_slice(&fs::read(output.join("receipt.json")).unwrap()).unwrap();
    assert_eq!(receipt["mismatch_count"], 0);
    assert_eq!(receipt["steps"], 1);
    for args in [
        vec!["--output", "--help"],
        vec!["--help", "--"],
        vec!["--binary", "--source"],
    ] {
        let result = std::process::Command::new(binary)
            .args(&args)
            .output()
            .unwrap();
        assert_eq!(
            result.status.code(),
            Some(if args[0] == "--help" { 0 } else { 2 })
        );
        assert_eq!(fs::read(output.join("stdout.json")).unwrap(), data);
    }
}
