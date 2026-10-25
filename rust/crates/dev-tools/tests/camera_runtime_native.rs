//! Actual original CLI admission and side effects, with Python absent from PATH.
#![cfg(feature = "camera-runtime-native")]
use serde_json::Value;
use std::{fs, path::Path, process::Command};
#[test]
fn original_cli_admission_and_verify_only_filesystem_effects() {
    let captures: Value = serde_json::from_str(include_str!(
        "fixtures/camera-runtime-check/cli-original.json"
    ))
    .unwrap();
    for case in captures["cases"].as_array().unwrap() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let output = root.join("out");
        if case["name"] == "verify-existing-empty" {
            fs::create_dir(&output).unwrap();
        }
        let args = case["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().replace("$ROOT", root.to_str().unwrap()))
            .collect::<Vec<_>>();
        let mut command = Command::new(env!("CARGO_BIN_EXE_check-camerad-runtime"));
        command
            .args(args)
            .current_dir(root)
            .env("PATH", "/nonexistent")
            .env(
                "OPENPILOT_RUST_ROOT",
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."),
            );
        if let Some(program) = std::env::var_os("OPENPILOT_CAPNP") {
            command.env("OPENPILOT_CAPNP", program);
        }
        let result = command.output().unwrap();
        assert_eq!(
            result.status.code(),
            case["exit"].as_i64().map(|n| n as i32),
            "{}: {}",
            case["name"],
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            output.exists(),
            case["output_exists"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
        let mut files = if output.exists() {
            fs::read_dir(output)
                .unwrap()
                .map(|entry| entry.unwrap().file_name().into_string().unwrap())
                .collect::<Vec<_>>()
        } else {
            vec![]
        };
        files.sort();
        assert_eq!(
            serde_json::to_value(files).unwrap(),
            case["files"],
            "{}",
            case["name"]
        );
    }
}

fn peer(mode: &str, input: Option<&[u8]>) -> Value {
    use std::io::Write;
    use std::process::Stdio;
    let temporary = tempfile::tempdir().unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_camera-runtime-test-peer"));
    command
        .arg(mode)
        .arg(temporary.path())
        .env("PATH", "/nonexistent")
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut process = command.spawn().unwrap();
    if let Some(bytes) = input {
        process.stdin.take().unwrap().write_all(bytes).unwrap();
    }
    let output = process.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{mode}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
#[test]
fn actual_native_collectors_preserve_msgq_vision_samples_and_logger_packet() {
    let report = peer("backend-ipc", None);
    assert_eq!(
        report,
        serde_json::json!({
            "message":[17,34,51],
            "image":{"frame_id":7,"buffer_frame_id":7,"sof":100,"eof":200,"slot":0,
                "width":128,"height":128,"stride":128,"length":24576,"samples":[64,64,64]},
            "log":[20,123,34,108,101,118,101,108,110,117,109,34,58,50,48,125],
            "namespace_removed":true,"endpoint_removed":true,
        })
    );
}
#[test]
fn native_owned_group_cleanup_preserves_stdin_and_reaps_live_or_exited_leader() {
    for mode in [
        "backend-live-child",
        "backend-exited-child",
        "backend-live-child-wait",
        "backend-exited-child-wait",
    ] {
        assert_eq!(
            peer(mode, Some(b"native-input\n")),
            serde_json::json!({
                "input":"native-input\n","leader_reaped":true,"descendant_stopped":true,
            })
        );
    }
}
#[test]
fn failed_namespace_acquisition_never_removes_an_unowned_namespace_or_endpoint() {
    assert_eq!(
        peer("namespace-collision", None),
        serde_json::json!({
            "namespace_preserved":true,"endpoint_preserved":true,
        })
    );
}

#[test]
fn native_batch_decoder_preserves_original_event_projection_and_first_error() {
    use flate2::read::GzDecoder;
    use openpilot_dev_tools::camera_state_check::{NativeSchemaDecoder, SchemaDecoder};
    let fixtures =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/camera-runtime-check");
    let original: Value = serde_json::from_reader(GzDecoder::new(
        fs::File::open(fixtures.join("term-original.json.gz")).unwrap(),
    ))
    .unwrap();
    let messages: Value =
        serde_json::from_str(original["files"]["messages.json"].as_str().unwrap()).unwrap();
    let rows = messages
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|port| port.as_array().unwrap().iter().take(3))
        .collect::<Vec<_>>();
    let decode_hex = |hex: &str| {
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect::<Vec<_>>()
    };
    let mut wires = rows
        .iter()
        .map(|row| decode_hex(row["wire"].as_str().unwrap()))
        .collect::<Vec<_>>();
    // from_bytes ignores complete trailing words within each individual frame.
    wires[0].extend_from_slice(&[0; 8]);
    let program = std::env::var_os("OPENPILOT_CAPNP").unwrap_or_else(|| "capnp".into());
    let mut decoder = NativeSchemaDecoder::new(program.into());
    decoder.load(&fixtures.join("schema/log.capnp")).unwrap();
    let decoded = decoder.decode_many(&wires);
    assert_eq!(decoded.len(), rows.len());
    for (actual, expected) in decoded.into_iter().zip(rows) {
        let actual = actual.unwrap();
        assert!(actual.serialization_error.is_none());
        assert_eq!(
            serde_json::from_str::<Value>(&actual.value.to_json().unwrap()).unwrap(),
            expected["event"]
        );
    }
    assert!(decoder.decode_many(&[]).is_empty());
    let malformed = vec![wires[0].clone(), vec![1], wires[1].clone()];
    let mut results = decoder.decode_many(&malformed).into_iter();
    assert!(results.next().unwrap().is_ok());
    assert_eq!(
        results.next().unwrap().err().unwrap().to_string(),
        decoder.decode(&[1]).err().unwrap().to_string()
    );
    assert!(results.next().is_none());

    // This is a structurally complete message with an invalid root pointer:
    // external capnp emits partial JSON and exits nonzero. The original Python
    // decoder rejects it, so keep the valid prefix and stop at that exact row.
    let failure: Value = serde_json::from_str(include_str!(
        "fixtures/camera-runtime-check/batch-failure-original.json"
    ))
    .unwrap();
    assert_eq!(failure["cases"][1]["error"], "KjException");
    assert_eq!(failure["batch_exit"], 1);
    let valid = decode_hex(failure["cases"][0]["wire"].as_str().unwrap());
    let invalid = decode_hex(failure["cases"][1]["wire"].as_str().unwrap());
    let individual_error = decoder.decode(&invalid).err().unwrap().to_string();
    let mut decoded = decoder
        .decode_many(&[valid.clone(), invalid, valid])
        .into_iter();
    let first = decoded.next().unwrap().unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&first.value.to_json().unwrap()).unwrap(),
        failure["cases"][0]["original"]
    );
    assert_eq!(
        decoded.next().unwrap().err().unwrap().to_string(),
        individual_error
    );
    assert!(decoded.next().is_none());
}
