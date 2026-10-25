//! Raw stdin expectations come from the unchanged original Worker, not Session.
use super::{source, Session};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Cursor};

fn unhex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

#[test]
fn original_raw_stdin_surrogateescape_recovers_after_invalid_json_line() {
    // Strict UTF-8 decoding at the stdin boundary must fail this comparison:
    // an invalid byte inside a JSON string is preserved before safe filtering.
    let bytes = include_bytes!("../../tests/fixtures/tombstone-tools/raw-stdin-original.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "8460869f1ce3bec5508a1f5bbe4fa1d775c2a5d7e749e35424dd31697f6b052e"
    );
    let capture: serde_json::Value = serde_json::from_slice(bytes).unwrap();
    assert_eq!(capture["exit"], 0);
    assert_eq!(capture["stderr_hex"], "");
    assert_eq!(
        capture["original_source_sha256"],
        "7b647dba322d93e5d2bd9c5692356b10d3f85f2a2e0f9da8b982e05206c8e781"
    );
    let input = unhex(capture["stdin_hex"].as_str().unwrap());
    let expected = unhex(capture["stdout_hex"].as_str().unwrap());
    let temp = tempfile::tempdir().unwrap();
    let mut actual = Vec::new();
    source(
        temp.path(),
        &BTreeMap::new(),
        &mut Cursor::new(input),
        &mut actual,
    )
    .unwrap();
    let records = |bytes: &[u8]| {
        bytes
            .split(|byte| *byte == b'\n')
            .filter(|row| !row.is_empty())
            .map(|row| serde_json::from_slice::<serde_json::Value>(row).unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(records(&expected).len(), 3);
    assert_eq!(records(&actual), records(&expected));
}

#[test]
fn byte_request_api_preserves_strict_utf8_error_and_recovers() {
    let temp = tempfile::tempdir().unwrap();
    let mut session = Session::new(temp.path(), &BTreeMap::new()).unwrap();
    let response = session.request(b"\xff").unwrap();
    assert!(response
        .get("result")
        .unwrap()
        .get("error")
        .unwrap()
        .text_eq("UnicodeDecodeError"));
    let response = session
        .request(br#"{"op":"safe","text":"recovery/test"}"#)
        .unwrap();
    assert!(response
        .get("result")
        .unwrap()
        .get("value")
        .unwrap()
        .text_eq("recoverytest"));
}
