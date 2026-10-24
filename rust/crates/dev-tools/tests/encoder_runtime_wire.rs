//! Actual full-cereal messages, projected by the unchanged original Peer.drain.
#![cfg(feature = "encoder-runtime-native")]
use openpilot_dev_tools::encoder_runtime_wire::drain;
use serde_json::Value;
fn bytes(value: &Value) -> Vec<u8> {
    value
        .as_str()
        .unwrap()
        .as_bytes()
        .chunks_exact(2)
        .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn original_full_schema_packets_preserve_data_metadata_rejections_and_partial_raw() {
    let captured = include_str!("fixtures/encoder-runtime-check/wire-original.json");
    let corpus: Value = serde_json::from_str(captured).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    use sha2::{Digest, Sha256};
    assert_eq!(
        format!("{:x}", Sha256::digest(captured.as_bytes())),
        "caa0642155d824a31ded31ddcf35ba9fa5a473e1d8112ac2c55d1e10bb3ccfa3"
    );
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(std::fs::read(root.join("openpilot/cereal/log.capnp")).unwrap())
        ),
        corpus["schema_sha256"].as_str().unwrap()
    );
    // This specific oracle was retired after the captured contracts and actual
    // native peer passed. A changed reintroduction still invalidates provenance.
    match std::fs::read(root.join("rust/tools/check_encoder_runtime.py")) {
        Ok(source) => assert_eq!(
            format!("{:x}", Sha256::digest(source)),
            corpus["source_sha256"].as_str().unwrap()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => panic!("cannot inspect retired encoder oracle: {error}"),
    }
    assert_eq!(corpus["cases"].as_array().unwrap().len(), 26);
    for case in corpus["cases"].as_array().unwrap() {
        let service = case["service"].as_str().unwrap();
        let wires: Vec<_> = case["wires"]
            .as_array()
            .unwrap()
            .iter()
            .map(bytes)
            .collect();
        let mut raw = vec![];
        let mut rows = vec![];
        let result = drain(service, &wires, &mut raw, &mut rows);
        assert_eq!(
            result.is_err(),
            !case["error"].is_null(),
            "{}: {result:?}",
            case["name"]
        );
        if let Err(error) = result {
            if case["error"]["type"] != "KjException" {
                assert_eq!(
                    error.to_string(),
                    case["error"]["message"].as_str().unwrap(),
                    "{}",
                    case["name"]
                );
            }
        }
        assert_eq!(
            serde_json::to_value(&rows).unwrap(),
            case["rows"][service],
            "{}",
            case["name"]
        );
        assert_eq!(raw, bytes(&case["raw_hex"]), "{}", case["name"]);
    }
}
