#![cfg(all(target_os = "linux", feature = "changed-files-native"))]
use openpilot_dev_tools::changed_files_check::{decode_bytes, Phase};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::PathBuf};

// This NEW independent original capture is not interchangeable with the earlier
// names/UTF7 corpus. The parent pins its actual returned manifest in hosted CI.
#[derive(Deserialize)]
struct Seal {
    bytes: u64,
    sha256: String,
}
#[derive(Deserialize)]
struct Counts {
    rows: usize,
    accepted: usize,
    rejected: usize,
}
#[derive(Deserialize)]
struct Manifest {
    schema: u32,
    files: BTreeMap<String, Seal>,
    counts: BTreeMap<String, Counts>,
}
#[derive(Deserialize)]
struct Case {
    id: String,
    codec: String,
    source_hex: String,
    accepted: bool,
    #[serde(default)]
    utf8_hex: Option<String>,
}
#[derive(Deserialize)]
struct Corpus {
    family: String,
    rows: Vec<Case>,
}
fn digest(raw: &[u8]) -> String {
    format!("{:x}", Sha256::digest(raw))
}
fn unhex(raw: &str) -> Vec<u8> {
    assert_eq!(raw.len() % 2, 0);
    raw.as_bytes()
        .chunks_exact(2)
        .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
        .collect()
}
fn to_hex(raw: &[u8]) -> String {
    raw.iter().map(|v| format!("{v:02x}")).collect()
}
fn compare_family(family: &str, codecs: &[&str]) {
    let root = PathBuf::from(
        std::env::var_os("AST_CODEC_PAYLOAD_ORIGINAL_DIR")
            .expect("authenticated new original payload capture required"),
    );
    let pin = std::env::var("AST_CODEC_PAYLOAD_MANIFEST_SHA256")
        .expect("parent must pin the actual independently captured manifest");
    assert_eq!(pin.len(), 64);
    assert!(pin.bytes().all(|b| b.is_ascii_hexdigit()));
    let raw_manifest = fs::read(root.join("manifest.json")).unwrap();
    assert_eq!(
        digest(&raw_manifest),
        pin,
        "immutable new original manifest"
    );
    let manifest: Manifest = serde_json::from_slice(&raw_manifest).unwrap();
    assert_eq!(manifest.schema, 1);
    assert_eq!(manifest.counts.len(), 3);
    let seal = manifest.files.get(&format!("{family}.json")).unwrap();
    let raw = fs::read(root.join(format!("{family}.json"))).unwrap();
    assert_eq!(raw.len() as u64, seal.bytes);
    assert_eq!(digest(&raw), seal.sha256, "immutable original payload file");
    let original: Corpus = serde_json::from_slice(&raw).unwrap();
    assert_eq!(original.family, family);
    let count = manifest.counts.get(family).unwrap();
    assert_eq!(original.rows.len(), count.rows);
    let positive = original.rows.iter().filter(|r| r.accepted).count();
    assert_eq!(positive, count.accepted);
    assert_eq!(original.rows.len() - positive, count.rejected);
    assert!(positive >= 10 && count.rejected >= 3);
    let mut differences = 0usize;
    let mut actual_rows = Vec::with_capacity(original.rows.len());
    for case in &original.rows {
        assert!(codecs.contains(&case.codec.as_str()));
        assert_eq!(case.accepted, case.utf8_hex.is_some());
        let actual = decode_bytes(&unhex(&case.source_hex), &case.codec);
        let capability = actual.as_ref().is_err_and(|e| e.phase == Phase::Capability);
        let output_hex = actual.as_ref().ok().map(|s| to_hex(s.as_bytes()));
        let matches = !capability && actual.is_ok() == case.accepted && output_hex == case.utf8_hex;
        differences += usize::from(!matches);
        actual_rows.push(json!({
            "id":case.id,"codec":case.codec,"source_hex":case.source_hex,
            "original_accepted":case.accepted,"original_utf8_hex":case.utf8_hex,
            "native_accepted":actual.is_ok(),"native_utf8_hex":output_hex,
            "capability_error":capability,"matched":matches,
            "native_diagnostic":actual.err().map(|e|e.to_string())
        }));
    }
    let output = PathBuf::from(
        std::env::var_os("AST_CODEC_PAYLOAD_PROOF")
            .expect("retained actual payload output directory required"),
    );
    fs::create_dir_all(&output).unwrap();
    fs::write(
        output.join(format!("{family}.json")),
        serde_json::to_vec(&json!({
            "family":family,"rows":actual_rows,"differences":differences,
            "original_manifest_sha256":pin,"original_file_sha256":seal.sha256,
            "original_accepted":positive,"original_rejected":count.rejected
        }))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(differences, 0, "exact original {family} payload mismatches");
}
#[test]
fn utf16_payloads_preserve_original_bom_endian_and_scalar_contract() {
    compare_family("utf16", &["utf-16", "utf-16-le", "utf-16-be"]);
}
#[test]
fn utf32_payloads_preserve_original_bom_endian_and_scalar_contract() {
    compare_family("utf32", &["utf-32", "utf-32-le", "utf-32-be"]);
}
#[test]
fn punycode_payloads_preserve_original_insertion_and_strict_utf8_contract() {
    compare_family("punycode", &["punycode"]);
}
