#![cfg(all(target_os = "linux", feature = "changed-files-native"))]
//! External original observations only: never read native mapping tables here.
use openpilot_dev_tools::changed_files_check::{decode_bytes, Phase};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::PathBuf};

fn digest(raw: &[u8]) -> String {
    format!("{:x}", Sha256::digest(raw))
}
#[derive(Deserialize)]
struct Seal {
    bytes: usize,
    sha256: String,
}
#[derive(Deserialize)]
struct Manifest {
    files: BTreeMap<String, Seal>,
}
struct Original {
    root: PathBuf,
    manifest: Manifest,
}
impl Original {
    fn open() -> Self {
        let root = PathBuf::from(
            std::env::var_os("JAPANESE_ORIGINAL_DIR")
                .expect("authenticated original artifact directory"),
        );
        let expected = std::env::var("JAPANESE_ORIGINAL_MANIFEST_SHA256")
            .expect("parent's independently frozen original manifest hash");
        assert_eq!(expected.len(), 64);
        assert!(expected
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
        let bytes = fs::read(root.join("manifest.json")).unwrap();
        assert_eq!(
            digest(&bytes),
            expected,
            "original manifest bound before native execution"
        );
        let manifest: Manifest = serde_json::from_slice(&bytes).unwrap();
        Self { root, manifest }
    }
    fn read(&self, name: &str) -> Vec<u8> {
        assert!(!name.is_empty() && !name.contains(['/', '\\']) && name != "." && name != "..");
        let seal = self.manifest.files.get(name).expect("original file seal");
        let raw = fs::read(self.root.join(name)).unwrap();
        assert_eq!(raw.len(), seal.bytes, "{name}");
        assert_eq!(digest(&raw), seal.sha256, "original hash {name}");
        raw
    }
}
fn save(name: &str, raw: &[u8]) {
    let root =
        PathBuf::from(std::env::var_os("JAPANESE_NATIVE_OUTPUT").expect("native output directory"));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join(name), raw).unwrap();
}
fn save_json(name: &str, data: &Value) {
    save(name, &serde_json::to_vec(data).unwrap());
}
fn hex(raw: &str) -> Vec<u8> {
    assert!(raw.len().is_multiple_of(2));
    raw.as_bytes()
        .chunks_exact(2)
        .map(|part| u8::from_str_radix(std::str::from_utf8(part).unwrap(), 16).unwrap())
        .collect()
}
fn hex_bytes(raw: &[u8]) -> String {
    raw.iter().map(|b| format!("{b:02x}")).collect()
}
#[derive(Deserialize)]
struct Domain {
    name: String,
    codec: String,
    prefix_hex: String,
    suffix_hex: String,
    width: usize,
    count: usize,
    record_bytes: usize,
}
#[derive(Deserialize)]
struct Domains {
    record_bytes: usize,
    rows: Vec<Domain>,
}
fn supported(codec: &str) -> bool {
    matches!(
        codec,
        "euc_jp"
            | "euc_jis_2004"
            | "euc_jisx0213"
            | "shift_jis_2004"
            | "shift_jisx0213"
            | "iso2022_jp"
            | "iso2022_jp_1"
            | "iso2022_jp_3"
            | "iso2022_jp_2004"
            | "iso2022_jp_ext"
    )
}
#[test]
fn independent_original_complete_japanese_domains_match_exact_payloads() {
    let original = Original::open();
    let receipt: Value = serde_json::from_slice(&original.read("receipt.json")).unwrap();
    assert_eq!(receipt["native_candidate_execution"], false);
    assert_eq!(receipt["native_expected_data_used"], false);
    assert_eq!(receipt["source_mapping_data_used_as_oracle"], false);
    assert_eq!(receipt["original_domain_rows"], 4_473_153);
    let domains: Domains = serde_json::from_slice(&original.read("domains.json")).unwrap();
    assert_eq!(domains.record_bytes, 10);
    assert_eq!(domains.rows.len(), 198);
    let mut differences = 0usize;
    let mut capabilities = 0usize;
    let mut first = Vec::new();
    let mut total = 0usize;
    for domain in domains.rows {
        assert!(supported(&domain.codec));
        assert!(domain.width <= 2);
        assert_eq!(
            domain.count,
            256usize.pow(u32::try_from(domain.width).unwrap())
        );
        assert_eq!(domain.record_bytes, 10);
        let expected = original.read(&domain.name);
        assert_eq!(expected.len(), domain.count * 10);
        let prefix = hex(&domain.prefix_hex);
        let suffix = hex(&domain.suffix_hex);
        let mut output = Vec::with_capacity(expected.len());
        let mut domain_differences = 0usize;
        for (index, expected) in expected.chunks_exact(10).enumerate() {
            assert!(expected[0] <= 1);
            let length = usize::from(expected[1]);
            assert!(length <= 8);
            assert!(expected[2 + length..].iter().all(|&b| b == 0));
            assert!(expected[0] == 1 || length == 0);
            let mut raw = prefix.clone();
            let bytes = u16::try_from(index).unwrap().to_be_bytes();
            raw.extend_from_slice(&bytes[2 - domain.width..]);
            raw.extend_from_slice(&suffix);
            let actual = decode_bytes(&raw, &domain.codec);
            let capability = actual.as_ref().is_err_and(|e| e.phase == Phase::Capability);
            capabilities += usize::from(capability);
            let mut native = [0u8; 10];
            if let Ok(text) = &actual {
                assert!(
                    text.len() <= 8,
                    "{} {} expansion",
                    domain.codec,
                    hex_bytes(&raw)
                );
                native[0] = 1;
                native[1] = u8::try_from(text.len()).unwrap();
                native[2..2 + text.len()].copy_from_slice(text.as_bytes());
            } else if capability {
                native[0] = 2;
            }
            if native.as_slice() != expected {
                domain_differences += 1;
                if first.len() < 100 {
                    first.push(json!({"domain":domain.name,"codec":domain.codec,"index":index,"source_hex":hex_bytes(&raw),"original_record_hex":hex_bytes(expected),"native_record_hex":hex_bytes(&native),"native_diagnostic":actual.err().map(|e|e.to_string())}));
                }
            }
            output.extend_from_slice(&native);
            total += 1;
        }
        save(&domain.name, &output);
        save_json(
            &format!("{}.summary.json", domain.name),
            &json!({"rows":domain.count,"differences":domain_differences,"native_sha256":digest(&output),"original_sha256":digest(&expected)}),
        );
        differences += domain_differences;
    }
    save_json(
        "domains-summary.json",
        &json!({"rows":total,"differences":differences,"capability_errors":capabilities,"first_differences":first}),
    );
    assert_eq!(total, 4_473_153);
    assert_eq!(
        capabilities, 0,
        "every capability remains an unclosed error"
    );
    assert_eq!(
        differences, 0,
        "complete actual native output retained before rejection"
    );
}
#[derive(Deserialize)]
struct Vector {
    codec: String,
    hex: String,
    accepted: bool,
    utf8_hex: Option<String>,
    native_patch_scope: bool,
}
#[derive(Deserialize)]
struct Vectors {
    rows: Vec<Vector>,
}
#[test]
fn independent_original_variant_state_and_truncation_boundaries_match() {
    let original = Original::open();
    let vectors: Vectors = serde_json::from_slice(&original.read("vectors.json")).unwrap();
    let mut actual_rows = Vec::new();
    let mut differences = 0;
    for row in vectors.rows {
        if !row.native_patch_scope {
            continue;
        }
        assert!(supported(&row.codec));
        let raw = hex(&row.hex);
        let actual = decode_bytes(&raw, &row.codec);
        let capability = actual.as_ref().is_err_and(|e| e.phase == Phase::Capability);
        let utf8_hex = actual.as_ref().ok().map(|v| hex_bytes(v.as_bytes()));
        let matched = !capability && actual.is_ok() == row.accepted && utf8_hex == row.utf8_hex;
        differences += usize::from(!matched);
        actual_rows.push(json!({"codec":row.codec,"source_hex":row.hex,"accepted":actual.is_ok(),"utf8_hex":utf8_hex,"capability":capability,"diagnostic":actual.err().map(|e|e.to_string()),"matched":matched}));
    }
    assert_eq!(actual_rows.len(), 3550);
    save_json(
        "vectors-actual.json",
        &json!({"rows":actual_rows,"differences":differences}),
    );
    assert_eq!(differences, 0);
}
