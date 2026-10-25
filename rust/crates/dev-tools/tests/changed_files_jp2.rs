#![cfg(all(target_os = "linux", feature = "changed-files-native"))]
//! Authenticate separately captured original observations; never load native maps.
use openpilot_dev_tools::changed_files_check::{check_bytes, decode_bytes, Phase};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
};
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
    fn open(prefix: &str) -> Self {
        let root = PathBuf::from(
            std::env::var_os(format!("{prefix}_DIR")).expect("authenticated original directory"),
        );
        let pin = std::env::var(format!("{prefix}_MANIFEST_SHA256"))
            .expect("parent frozen original manifest hash");
        assert_eq!(pin.len(), 64);
        assert!(pin
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
        let raw = fs::read(root.join("manifest.json")).unwrap();
        assert_eq!(digest(&raw), pin);
        Self {
            root,
            manifest: serde_json::from_slice(&raw).unwrap(),
        }
    }
    fn read(&self, name: &str) -> Vec<u8> {
        assert!(
            !name.is_empty()
                && !name.contains('/')
                && !name.contains('\\')
                && name != "."
                && name != ".."
        );
        let seal = self.manifest.files.get(name).expect("original seal");
        let raw = fs::read(self.root.join(name)).unwrap();
        assert_eq!(raw.len(), seal.bytes);
        assert_eq!(digest(&raw), seal.sha256);
        raw
    }
}
fn save(name: &str, raw: &[u8]) {
    let root =
        PathBuf::from(std::env::var_os("JP2_NATIVE_OUTPUT").expect("native output directory"));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join(name), raw).unwrap();
}
fn save_json(name: &str, value: &Value) {
    save(name, &serde_json::to_vec(value).unwrap());
}
fn hex(raw: &str) -> Vec<u8> {
    assert!(raw.len().is_multiple_of(2));
    raw.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
fn bytes_hex(raw: &[u8]) -> String {
    raw.iter().map(|b| format!("{b:02x}")).collect()
}
#[derive(Deserialize)]
struct Payload {
    hex: String,
    accepted: bool,
    utf8_hex: Option<String>,
}
#[derive(Deserialize)]
struct Previous {
    codec: String,
    hex: String,
    accepted: bool,
    utf8_hex: Option<String>,
}
#[derive(Deserialize)]
struct Rows<T> {
    rows: Vec<T>,
}
fn payload_row(row: &Payload) -> (Value, bool) {
    let actual = decode_bytes(&hex(&row.hex), "iso2022_jp_2");
    let capability = actual.as_ref().is_err_and(|e| e.phase == Phase::Capability);
    let utf8_hex = actual.as_ref().ok().map(|s| bytes_hex(s.as_bytes()));
    let matched = !capability && actual.is_ok() == row.accepted && utf8_hex == row.utf8_hex;
    (
        json!({"source_hex":row.hex,"accepted":actual.is_ok(),"utf8_hex":utf8_hex,"capability":capability,"diagnostic":actual.err().map(|e|e.to_string()),"matched":matched}),
        matched,
    )
}
#[derive(Deserialize)]
struct Domain {
    name: String,
    prefix_hex: String,
    suffix_hex: String,
    width: usize,
    count: usize,
    skipped_indices: Vec<usize>,
    captured: usize,
}
#[test]
fn original_jp2_payload_domains_reuse_prior_observations_and_compare_new_gaps() {
    let prior = Original::open("JP2_PRIOR_ORIGINAL");
    let original = Original::open("JP2_ORIGINAL");
    let receipt: Value = serde_json::from_slice(&original.read("receipt.json")).unwrap();
    assert_eq!(receipt["native_candidate_execution"], false);
    assert_eq!(receipt["native_tables_used_as_expected"], false);
    assert_eq!(receipt["prior_inputs_recaptured"], false);
    let roster: Value = serde_json::from_slice(&original.read("reused-jp2-inputs.json")).unwrap();
    let roster: BTreeSet<Vec<u8>> = roster["hex"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| hex(v.as_str().unwrap()))
        .collect();
    assert_eq!(roster.len(), 45);
    let previous: Rows<Previous> = serde_json::from_slice(&prior.read("vectors.json")).unwrap();
    let mut seen = BTreeSet::new();
    let mut rows = Vec::new();
    let mut differences = 0usize;
    for row in previous
        .rows
        .into_iter()
        .filter(|r| r.codec == "iso2022_jp_2")
    {
        assert!(seen.insert(hex(&row.hex)));
        let (value, matched) = payload_row(&Payload {
            hex: row.hex,
            accepted: row.accepted,
            utf8_hex: row.utf8_hex,
        });
        rows.push(value);
        differences += usize::from(!matched);
    }
    assert_eq!(seen, roster);
    save_json(
        "prior-vectors.actual.json",
        &json!({"rows":rows,"differences":differences}),
    );
    let domains: Rows<Domain> = serde_json::from_slice(&original.read("domains.json")).unwrap();
    assert_eq!(domains.rows.len(), 45);
    let mut total = 0usize;
    let mut first = Vec::new();
    let mut capabilities = 0usize;
    for domain in domains.rows {
        assert!(domain.width <= 2);
        assert_eq!(
            domain.count,
            256usize.pow(u32::try_from(domain.width).unwrap())
        );
        let expected = original.read(&domain.name);
        assert_eq!(expected.len(), domain.captured * 10);
        let skipped: BTreeSet<usize> = domain.skipped_indices.into_iter().collect();
        assert_eq!(domain.captured + skipped.len(), domain.count);
        let prefix = hex(&domain.prefix_hex);
        let suffix = hex(&domain.suffix_hex);
        let mut at = 0usize;
        let mut actual_rows = Vec::new();
        for index in 0..domain.count {
            let mut raw = prefix.clone();
            raw.extend_from_slice(&u16::try_from(index).unwrap().to_be_bytes()[2 - domain.width..]);
            raw.extend_from_slice(&suffix);
            let new = seen.insert(raw.clone());
            assert_eq!(
                !new,
                skipped.contains(&index),
                "only previously observed exact inputs may be skipped"
            );
            if !new {
                continue;
            }
            let expected = &expected[at..at + 10];
            at += 10;
            assert!(expected[0] <= 1);
            assert!(expected[1] <= 8);
            assert!(expected[2 + usize::from(expected[1])..]
                .iter()
                .all(|&b| b == 0));
            let actual = decode_bytes(&raw, "iso2022_jp_2");
            let capability = actual.as_ref().is_err_and(|e| e.phase == Phase::Capability);
            capabilities += usize::from(capability);
            let mut record = [0u8; 10];
            if let Ok(text) = &actual {
                assert!(text.len() <= 8);
                record[0] = 1;
                record[1] = u8::try_from(text.len()).unwrap();
                record[2..2 + text.len()].copy_from_slice(text.as_bytes());
            } else if capability {
                record[0] = 2;
            }
            if record.as_slice() != expected {
                differences += 1;
                if first.len() < 100 {
                    first.push(json!({"domain":domain.name,"index":index,"source_hex":bytes_hex(&raw),"original_record_hex":bytes_hex(expected),"native_record_hex":bytes_hex(&record),"diagnostic":actual.err().map(|e|e.to_string())}));
                }
            }
            actual_rows.extend_from_slice(&record);
            total += 1;
        }
        assert_eq!(at, expected.len());
        save(&domain.name, &actual_rows);
    }
    let vectors: Rows<Payload> = serde_json::from_slice(&original.read("vectors.json")).unwrap();
    let count = vectors.rows.len();
    let mut actual_rows = Vec::new();
    for row in vectors.rows {
        assert!(seen.insert(hex(&row.hex)));
        let (value, matched) = payload_row(&row);
        differences += usize::from(!matched);
        actual_rows.push(value);
    }
    save_json("vectors.actual.json", &json!({"rows":actual_rows}));
    save_json(
        "payload-summary.json",
        &json!({"reused_original_inputs":45,"dense_rows":total,"new_vectors":count,"differences":differences,"capability_errors":capabilities,"first_differences":first}),
    );
    assert_eq!(json!(total), receipt["dense_rows"]);
    assert_eq!(json!(count), receipt["payload_vector_rows"]);
    assert_eq!(capabilities, 0);
    assert_eq!(differences, 0, "all native actuals saved before assertion");
}
#[derive(Deserialize)]
struct Source {
    source_hex: String,
    accepted: bool,
}
#[test]
fn original_jp2_raw_source_admission_matches_maintained_ast_parse_contract() {
    let original = Original::open("JP2_ORIGINAL");
    let rows: Rows<Source> =
        serde_json::from_slice(&original.read("source-admission.json")).unwrap();
    assert_eq!(rows.rows.len(), 101);
    let mut actual_rows = Vec::new();
    let mut differences = 0usize;
    for row in rows.rows {
        let actual = check_bytes(&hex(&row.source_hex));
        let capability = actual.as_ref().is_err_and(|e| e.phase == Phase::Capability);
        let matched = !capability && actual.is_ok() == row.accepted;
        differences += usize::from(!matched);
        actual_rows.push(json!({"source_hex":row.source_hex,"native_accepted":actual.is_ok(),"original_accepted":row.accepted,"capability":capability,"diagnostic":actual.err().map(|e|e.to_string()),"matched":matched}));
    }
    save_json(
        "source-admission.actual.json",
        &json!({"rows":actual_rows,"differences":differences}),
    );
    assert_eq!(differences, 0);
}
