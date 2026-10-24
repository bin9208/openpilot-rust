//! Run only after independently capturing and sealing the original hosted oracle.
use super::*;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::PathBuf};

#[derive(Deserialize)]
struct Seal {
    bytes: u64,
    sha256: String,
}
#[derive(Deserialize)]
struct Manifest {
    files: BTreeMap<String, Seal>,
}
fn digest(raw: &[u8]) -> String {
    format!("{:x}", Sha256::digest(raw))
}
fn original(name: &str) -> Vec<u8> {
    let root =
        PathBuf::from(std::env::var_os("IDNA_ORIGINAL_DIR").expect("sealed original directory"));
    let manifest = fs::read(root.join("manifest.json")).unwrap();
    // Parent freezes this independently audited SHA into the hosted workflow.
    let pin = std::env::var("IDNA_ORIGINAL_MANIFEST_SHA").expect("parent's immutable original SHA");
    assert_eq!(pin.len(), 64);
    assert_eq!(digest(&manifest), pin);
    let manifest: Manifest = serde_json::from_slice(&manifest).unwrap();
    let pin = manifest.files.get(name).unwrap();
    let bytes = fs::read(root.join(name)).unwrap();
    assert_eq!(bytes.len() as u64, pin.bytes);
    assert_eq!(digest(&bytes), pin.sha256);
    bytes
}
fn save(name: &str, bytes: &[u8]) {
    let root =
        PathBuf::from(std::env::var_os("IDNA_ACTUAL_DIR").expect("native actual output directory"));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join(name), bytes).unwrap();
}
fn take_u32(raw: &mut &[u8]) -> u32 {
    let (head, tail) = raw.split_at(4);
    *raw = tail;
    u32::from_le_bytes(head.try_into().unwrap())
}
fn take_blob<'a>(raw: &mut &'a [u8]) -> Option<&'a [u8]> {
    let len = take_u32(raw);
    if len == u32::MAX {
        return None;
    }
    let (head, tail) = raw.split_at(len as usize);
    *raw = tail;
    Some(head)
}
fn append_blob(output: &mut Vec<u8>, value: Result<String, Diagnostic>) {
    if let Ok(text) = value {
        output.extend_from_slice(&u32::try_from(text.len()).unwrap().to_le_bytes());
        output.extend_from_slice(text.as_bytes());
    } else {
        output.extend_from_slice(&u32::MAX.to_le_bytes());
    }
}

#[test]
fn all_original_scalar_nameprep_and_ace_decodes_match_strict_utf8() {
    let bytes = original("idna-scalars.bin");
    let mut input = bytes.as_slice();
    assert_eq!(&input[..8], b"IDNA296\x01");
    input = &input[8..];
    assert_eq!(take_u32(&mut input), 1_112_064);
    let mut differences = 0usize;
    let mut first_differences = Vec::new();
    let mut actual = b"IDNA296\x01".to_vec();
    actual.extend_from_slice(&1_112_064u32.to_le_bytes());
    for cp in (0..0x110000).filter(|cp| !(0xd800..=0xdfff).contains(cp)) {
        assert_eq!(take_u32(&mut input), cp);
        // Input bytes come from original CPython Punycode, never native encode.
        let raw = take_blob(&mut input).unwrap();
        let original_prep = take_blob(&mut input);
        let original_decode = take_blob(&mut input);
        actual.extend_from_slice(&cp.to_le_bytes());
        actual.extend_from_slice(&u32::try_from(raw.len()).unwrap().to_le_bytes());
        actual.extend_from_slice(raw);
        let prep = nameprep(&char::from_u32(cp).unwrap().to_string());
        let decoded = decode(raw);
        let prep_bytes = prep.as_ref().ok().map(|value| value.as_bytes());
        let decoded_bytes = decoded.as_ref().ok().map(|value| value.as_bytes());
        let capability = prep.as_ref().is_err_and(|e| e.phase == Phase::Capability)
            || decoded
                .as_ref()
                .is_err_and(|e| e.phase == Phase::Capability);
        if capability || prep_bytes != original_prep || decoded_bytes != original_decode {
            differences += 1;
            if first_differences.len() < 32 {
                first_differences.push(serde_json::json!({"cp":cp,"input_hex":hex_bytes(raw),"original_prep":original_prep.map(hex_bytes),"native_prep":prep_bytes.map(hex_bytes),"original_decode":original_decode.map(hex_bytes),"native_decode":decoded_bytes.map(hex_bytes),"prep_diagnostic":prep.as_ref().err().map(|e|e.to_string()),"decode_diagnostic":decoded.as_ref().err().map(|e|e.to_string())}));
            }
        }
        append_blob(&mut actual, prep);
        append_blob(&mut actual, decoded);
    }
    assert!(input.is_empty());
    save("idna-scalars.actual.bin", &actual);
    save("idna-scalars.actual.sha256", digest(&actual).as_bytes());
    save("idna-scalars.summary.json", &serde_json::to_vec(&serde_json::json!({"rows":1_112_064,"differences":differences,"first_differences":first_differences,"original_sha256":digest(&bytes),"native_sha256":digest(&actual)})).unwrap());
    assert_eq!(
        differences, 0,
        "original scalar differences: inspect saved actual bytes and summary"
    );
    assert_eq!(
        digest(&actual),
        digest(&bytes),
        "full original binary identity"
    );
}
#[derive(Deserialize)]
struct Row {
    operation: String,
    input_hex: String,
    accepted: bool,
    utf8_hex: Option<String>,
}
#[derive(Deserialize)]
struct Vectors {
    rows: Vec<Row>,
}
fn hex(raw: &str) -> Vec<u8> {
    assert_eq!(raw.len() % 2, 0);
    raw.as_bytes()
        .chunks_exact(2)
        .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
        .collect()
}
fn hex_bytes(raw: &[u8]) -> String {
    raw.iter().map(|b| format!("{b:02x}")).collect()
}
#[test]
fn original_multichar_bidi_normalization_malformed_and_length_controls_match() {
    let expected: Vectors = serde_json::from_slice(&original("idna-vectors.json")).unwrap();
    assert!(!expected.rows.is_empty());
    let mut differences = 0usize;
    let mut actual = Vec::new();
    for row in expected.rows {
        let raw = hex(&row.input_hex);
        let result = match row.operation.as_str() {
            "decode" => super::super::decode_bytes(&raw, "idna"),
            "nameprep" => nameprep(std::str::from_utf8(&raw).unwrap()),
            _ => panic!("unknown original operation"),
        };
        let utf8_hex = result.as_ref().ok().map(|text| hex_bytes(text.as_bytes()));
        let capability = result.as_ref().is_err_and(|e| e.phase == Phase::Capability);
        let matched = !capability && result.is_ok() == row.accepted && utf8_hex == row.utf8_hex;
        differences += usize::from(!matched);
        actual.push(serde_json::json!({"operation":row.operation,"input_hex":row.input_hex,"accepted":result.is_ok(),"utf8_hex":utf8_hex,"capability_error":capability,"diagnostic":result.err().map(|e|e.to_string()),"matched":matched}));
    }
    save(
        "idna-vectors.actual.json",
        &serde_json::to_vec(&serde_json::json!({"rows":actual,"differences":differences})).unwrap(),
    );
    assert_eq!(differences, 0, "full actual vector outputs retained");
}
