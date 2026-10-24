#![cfg(feature = "bluetooth-input-native")]
use openpilot_dev_tools::bluetooth_input_check::{cases, expected};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::Read;

fn original() -> Vec<Value> {
    let compressed = include_bytes!("fixtures/bluetooth-input/original.json.gz");
    assert_eq!(
        format!("{:x}", Sha256::digest(compressed)),
        "3612a4b62004d4941e67f747923b5f5eab6da340e3da64ab0137471906018990"
    );
    let mut bytes = vec![];
    flate2::read::GzDecoder::new(compressed.as_slice())
        .read_to_end(&mut bytes)
        .unwrap();
    assert_eq!(bytes.len(), 1_870_189);
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        "871bd09a5323bf0dab2cf1f21035a968d3de3451750929ae37a4f3e8ed9f89af"
    );
    serde_json::from_slice(&bytes).unwrap()
}
#[test]
fn unchanged_original_schedule_signed_fields_and_binary64_bits() {
    let original = original();
    let inputs = cases();
    assert_eq!(inputs.len(), 10_049);
    assert_eq!(original.len(), inputs.len());
    for (i, (input, row)) in inputs.iter().zip(original).enumerate() {
        let hex = input.iter().map(|b| format!("{b:02x}")).collect::<String>();
        assert_eq!(hex, row["hex"], "case {i}");
        let observed = expected(input).unwrap();
        if let Some(bits) = row["at_bits"].as_str() {
            let actual = observed["events"][0]["at"].as_f64().unwrap().to_bits();
            assert_eq!(format!("{actual:016x}"), bits, "case {i}");
            for name in ["kind", "code", "value"] {
                assert_eq!(
                    observed["events"][0][name], row["expected"]["events"][0][name],
                    "case {i} {name}"
                );
            }
        } else {
            assert_eq!(observed, row["expected"], "case {i}");
        }
    }
}
#[test]
fn complete_multiple_records_and_partial_boundaries() {
    let inputs = cases();
    let mut data = inputs[0].clone();
    data.extend_from_slice(&inputs[24]);
    let expected_two = expected(&data).unwrap();
    assert_eq!(expected_two["events"].as_array().unwrap().len(), 2);
    assert_eq!(
        expected_two["events"][0],
        expected(&inputs[0]).unwrap()["events"][0]
    );
    assert_eq!(
        expected_two["events"][1],
        expected(&inputs[24]).unwrap()["events"][0]
    );
    for n in [0, 1, 23, 25, 47, 49] {
        assert_eq!(
            expected(&vec![0; n]).unwrap(),
            serde_json::json!({"error":"HID device disconnected or incomplete event"})
        );
    }
}

use openpilot_dev_tools::{
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use openpilot_log_format::{JsonValue, JsonView};
use std::{fs, path::Path, time::Duration};
struct Peer {
    mode: String,
    lines: Vec<String>,
    calls: usize,
}
impl Runner for Peer {
    fn run(&mut self, command: &CommandSpec) -> Result<Captured> {
        self.calls += 1;
        assert_eq!(command.timeout, Duration::from_secs(30));
        assert_eq!(command.effective_argv()[1], "decode");
        assert!(Path::new(&command.effective_argv()[0]).is_absolute());
        assert_eq!(
            format!("{:x}", Sha256::digest(command.input.as_ref().unwrap())),
            "ef3d099c789cf1663095861a6f09c1b9446f105338820146737921902d2c0f04"
        );
        if self.mode == "timeout" {
            return Err("fixture child deadline".into());
        }
        let mut lines = self.lines.clone();
        let mut code = 0;
        match self.mode.as_str() {
            "exit9" => {
                code = 9;
                lines = vec!["diagnostic output".into()];
            }
            "missing" => {
                lines.pop();
            }
            "extra" => lines.push(lines[0].clone()),
            "invalid-json" => lines = vec!["not JSON".into()],
            "boolean-zero" | "integer-zero" | "negative-zero" => {
                let mut row: Value = serde_json::from_str(&lines[12]).unwrap();
                row["events"][0]["at"] = match self.mode.as_str() {
                    "boolean-zero" => Value::Bool(false),
                    "integer-zero" => Value::from(0),
                    _ => Value::from(-0.0),
                };
                lines[12] = row.to_string();
            }
            "different-value" => lines[0] = lines[0].replace("-2147483648", "-2147483647"),
            "different-timestamp" => {
                let mut row: Value = serde_json::from_str(&lines[0]).unwrap();
                row["events"][0]["at"] = Value::from(f64::from_bits(
                    row["events"][0]["at"].as_f64().unwrap().to_bits() ^ 1,
                ));
                lines[0] = row.to_string();
            }
            _ => {}
        }
        Ok(Captured {
            code,
            stdout: if self.mode == "decode-error" {
                vec![255]
            } else {
                format!("{}\n", lines.join("\n")).into_bytes()
            },
            stderr: b"fixture stderr\n".to_vec(),
        })
    }
}
fn original_lines() -> Vec<String> {
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("fixtures/bluetooth-input/original.json.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let JsonView::Array(rows) = JsonValue::parse(&text).unwrap().view() else {
        panic!()
    };
    rows.iter()
        .map(|r| r.get("expected").unwrap().to_json().unwrap())
        .collect()
}
#[test]
fn original_child_json_cardinality_and_artifact_failure_phases() {
    let bytes = include_bytes!("fixtures/bluetooth-input/boundaries.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "421a023e7cbfa49c17589c7175d08a1aae52068df9db9a78a137c85332f00ea1"
    );
    let rows: Vec<Value> = serde_json::from_slice(bytes).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for row in rows {
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("candidate");
        let output = dir.path().join("evidence");
        let mode = row["mode"].as_str().unwrap();
        if mode != "missing-binary" {
            fs::write(&binary, b"fixture executable identity\n").unwrap();
        }
        if mode == "existing-output" {
            fs::create_dir(&output).unwrap();
        }
        let mut peer = Peer {
            mode: mode.into(),
            lines: original_lines(),
            calls: 0,
        };
        let result = openpilot_dev_tools::bluetooth_input_check::check(
            &root,
            &binary,
            &output,
            &mut peer,
            &mut vec![],
        );
        assert_eq!(result.is_ok(), row["error"].is_null(), "{mode}: {result:?}");
        assert_eq!(peer.calls, row["calls"].as_array().unwrap().len(), "{mode}");
        for name in [
            "input.jsonl",
            "native.jsonl",
            "native.stderr",
            "source.json",
            "result.json",
        ] {
            assert_eq!(
                output.join(name).exists(),
                row["artifacts"].get(name).is_some(),
                "{mode}: {name}"
            );
        }
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(fs::read(output.join("input.jsonl")).unwrap_or_default())
            ),
            if mode == "existing-output" {
                format!("{:x}", Sha256::digest(b""))
            } else {
                row["artifacts"]["input.jsonl"]["sha256"]
                    .as_str()
                    .unwrap()
                    .into()
            }
        );
        if result.is_ok() {
            for name in ["source.json", "result.json"] {
                assert_eq!(
                    format!("{:x}", Sha256::digest(fs::read(output.join(name)).unwrap())),
                    row["artifacts"][name]["sha256"],
                    "{name}"
                );
            }
        }
    }
}

#[path = "bluetooth_input_check/controls.rs"]
mod controls;
