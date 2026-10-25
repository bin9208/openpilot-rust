#![cfg(all(target_os = "linux", feature = "changed-files-native"))]
//! Differential against independently sealed original observations; no map oracle.
use openpilot_dev_tools::changed_files_check::{check_bytes, decode_bytes, Phase};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
};

const CODECS: [&str; 3] = ["gb2312", "gbk", "gb18030"];
const ROSTER_SHA256: &str = "3c367e51288e7a7cd31f7e3bc8ad0d75a5620ae6beddc90dbf8f3f996cd7c0c7";
fn digest(raw: &[u8]) -> String {
    format!("{:x}", Sha256::digest(raw))
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
fn save(name: &str, raw: &[u8]) {
    assert!(!name.contains('/') && !name.contains('\\'));
    let root =
        PathBuf::from(std::env::var_os("GB_NATIVE_OUTPUT").expect("native output directory"));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join(name), raw).unwrap();
}
fn save_json(name: &str, value: &Value) {
    save(name, &serde_json::to_vec(value).unwrap());
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
        let root = PathBuf::from(std::env::var_os("GB_ORIGINAL_DIR").expect("original directory"));
        let pin =
            std::env::var("GB_ORIGINAL_MANIFEST_SHA256").expect("parent frozen manifest hash");
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
        assert!(!name.is_empty() && !name.contains('/') && !name.contains('\\'));
        assert!(name != "." && name != "..");
        let seal = self.manifest.files.get(name).expect("original file seal");
        let raw = fs::read(self.root.join(name)).unwrap();
        assert_eq!(raw.len(), seal.bytes);
        assert_eq!(digest(&raw), seal.sha256);
        raw
    }
    fn receipt(&self) -> Value {
        let receipt: Value = serde_json::from_slice(&self.read("receipt.json")).unwrap();
        assert_eq!(receipt["passed"], true);
        assert_eq!(receipt["native_candidate_execution"], false);
        assert_eq!(receipt["native_tables_used_as_expected"], false);
        assert_eq!(receipt["prior_inputs_recaptured"], false);
        assert_eq!(receipt["prior_rows"], 135);
        assert_eq!(receipt["historical_cn_extension_seal_missing"], true);
        assert_eq!(receipt["domain_count"], 11);
        assert_eq!(receipt["unicode"], "15.0.0");
        assert_eq!(
            receipt["source"],
            std::env::var("GB_ORIGINAL_SOURCE_SHA").expect("parent frozen original source")
        );
        let before = self.read("providers.before.json");
        assert_eq!(before, self.read("providers.after.json"));
        let providers: BTreeMap<String, Seal> = serde_json::from_slice(&before).unwrap();
        for needle in ["_codecs_cn.", "_multibytecodec.", "/bin/python3.12"] {
            assert!(providers.keys().any(|p| p.contains(needle)));
        }
        receipt
    }
}
#[derive(Deserialize)]
struct Payload {
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
    assert!(CODECS.contains(&row.codec.as_str()));
    let actual = decode_bytes(&hex(&row.hex), &row.codec);
    let strict = actual
        .as_ref()
        .err()
        .is_none_or(|e| e.phase == Phase::Decode);
    let utf8_hex = actual.as_ref().ok().map(|s| bytes_hex(s.as_bytes()));
    let matched = strict && actual.is_ok() == row.accepted && utf8_hex == row.utf8_hex;
    (
        json!({"codec":row.codec,"hex":row.hex,"accepted":actual.is_ok(),"utf8_hex":utf8_hex,
               "strict_decode_result":strict,"diagnostic":actual.err().map(|e|e.to_string()),"matched":matched}),
        matched,
    )
}
#[derive(Deserialize)]
struct Domain {
    name: String,
    codec: String,
    kind: String,
    width: usize,
    count: usize,
    skipped_indices: Vec<usize>,
    captured: usize,
}
fn domain_input(domain: &Domain, index: usize) -> Vec<u8> {
    match domain.kind.as_str() {
        "dense" => u16::try_from(index).unwrap().to_be_bytes()[2 - domain.width..].to_vec(),
        "three-prefix" => vec![
            u8::try_from(index / 2560).unwrap() + 0x81,
            u8::try_from(index / 256 % 10).unwrap() + 0x30,
            u8::try_from(index % 256).unwrap(),
        ],
        "four-shape" => vec![
            u8::try_from(index / 12600).unwrap() + 0x81,
            u8::try_from(index / 1260 % 10).unwrap() + 0x30,
            u8::try_from(index / 10 % 126).unwrap() + 0x81,
            u8::try_from(index % 10).unwrap() + 0x30,
        ],
        _ => panic!("unreviewed original domain"),
    }
}
#[test]
fn original_gb_dense_and_boundary_payloads() {
    let original = Original::open();
    let receipt = original.receipt();
    let raw = original.read("reused-gb-inputs.json");
    assert_eq!(digest(&raw), ROSTER_SHA256);
    let prior: Rows<Payload> = serde_json::from_slice(&raw).unwrap();
    let mut previous = BTreeSet::new();
    let mut prior_actuals = Vec::new();
    let mut differences = 0usize;
    for row in prior.rows {
        assert!(previous.insert((row.codec.clone(), hex(&row.hex))));
        let (value, matched) = payload_row(&row);
        differences += usize::from(!matched);
        prior_actuals.push(value);
    }
    assert_eq!(previous.len(), 135);
    for codec in CODECS {
        assert_eq!(previous.iter().filter(|(c, _)| c == codec).count(), 45);
    }
    save_json("prior-vectors.actual.json", &json!({"rows":prior_actuals}));
    let raw = original.read("domains.json");
    let contract: Value = serde_json::from_slice(&raw).unwrap();
    assert_eq!(contract["record_bytes"], 10);
    let domains: Rows<Domain> = serde_json::from_slice(&raw).unwrap();
    assert_eq!(domains.rows.len(), 11);
    let mut actual_domains = BTreeSet::new();
    let mut first = Vec::new();
    let mut total = 0usize;
    let mut wrong_phase = 0usize;
    for (ordinal, domain) in domains.rows.into_iter().enumerate() {
        assert_eq!(domain.name, format!("gb-domain-{ordinal:02}.bin"));
        assert!(CODECS.contains(&domain.codec.as_str()));
        let (width, count) = match domain.kind.as_str() {
            "dense" => {
                assert!(domain.width <= 2);
                (
                    domain.width,
                    256usize.pow(u32::try_from(domain.width).unwrap()),
                )
            }
            "three-prefix" => {
                assert_eq!(domain.codec, "gb18030");
                (3, 322560)
            }
            "four-shape" => {
                assert_eq!(domain.codec, "gb18030");
                (4, 1587600)
            }
            _ => panic!("unreviewed original domain"),
        };
        assert_eq!((domain.width, domain.count), (width, count));
        assert!(actual_domains.insert((domain.codec.clone(), domain.kind.clone(), domain.width)));
        let expected = original.read(&domain.name);
        assert_eq!(expected.len(), domain.captured * 10);
        let skipped: BTreeSet<usize> = domain.skipped_indices.iter().copied().collect();
        assert!(skipped.iter().all(|&n| n < domain.count));
        assert_eq!(domain.captured + skipped.len(), domain.count);
        let mut at = 0;
        let mut actual_rows = Vec::with_capacity(expected.len());
        for index in 0..domain.count {
            let raw = domain_input(&domain, index);
            let old = previous.contains(&(domain.codec.clone(), raw.clone()));
            assert_eq!(
                old,
                skipped.contains(&index),
                "only exact old inputs may be skipped"
            );
            if old {
                continue;
            }
            let expected = &expected[at..at + 10];
            at += 10;
            assert!(expected[0] <= 1 && expected[1] <= 8);
            assert!(expected[2 + usize::from(expected[1])..]
                .iter()
                .all(|&b| b == 0));
            assert!(expected[0] == 1 || expected.iter().all(|&b| b == 0));
            let actual = decode_bytes(&raw, &domain.codec);
            let mut record = [0u8; 10];
            if let Ok(text) = &actual {
                assert!(text.len() <= 8);
                record[0] = 1;
                record[1] = u8::try_from(text.len()).unwrap();
                record[2..2 + text.len()].copy_from_slice(text.as_bytes());
            } else if actual.as_ref().err().unwrap().phase != Phase::Decode {
                wrong_phase += 1;
                record[0] = 2;
            }
            if record.as_slice() != expected {
                differences += 1;
                if first.len() < 100 {
                    first.push(json!({"domain":domain.name,"index":index,"codec":domain.codec,
                        "hex":bytes_hex(&raw),"original_record_hex":bytes_hex(expected),
                        "native_record_hex":bytes_hex(&record),"diagnostic":actual.err().map(|e|e.to_string())}));
                }
            }
            actual_rows.extend_from_slice(&record);
            total += 1;
        }
        assert_eq!(at, expected.len());
        save(&domain.name, &actual_rows);
    }
    for codec in CODECS {
        for width in 0..=2 {
            assert!(actual_domains.contains(&(codec.to_owned(), "dense".to_owned(), width)));
        }
    }
    assert!(actual_domains.contains(&("gb18030".to_owned(), "three-prefix".to_owned(), 3)));
    assert!(actual_domains.contains(&("gb18030".to_owned(), "four-shape".to_owned(), 4)));
    let vectors: Rows<Payload> = serde_json::from_slice(&original.read("vectors.json")).unwrap();
    let count = vectors.rows.len();
    let mut seen = previous;
    let mut rows = Vec::new();
    for row in vectors.rows {
        assert!(seen.insert((row.codec.clone(), hex(&row.hex))));
        let (value, matched) = payload_row(&row);
        differences += usize::from(!matched);
        rows.push(value);
    }
    save_json("vectors.actual.json", &json!({"rows":rows}));
    save_json(
        "payload-summary.json",
        &json!({"dense_rows":total,"new_vectors":count,
        "reused_original_inputs":135,"differences":differences,"wrong_error_phase":wrong_phase,
        "historical_cn_extension_seal_missing":true,"first_differences":first}),
    );
    assert_eq!(json!(total), receipt["dense_rows"]);
    assert_eq!(json!(count), receipt["payload_vector_rows"]);
    assert_eq!(wrong_phase, 0);
    assert_eq!(
        differences, 0,
        "all native actuals saved before asserting parity"
    );
}
#[derive(Deserialize)]
struct Source {
    source_hex: String,
    accepted: bool,
}
#[test]
fn original_gb_raw_source_admission() {
    let original = Original::open();
    let receipt = original.receipt();
    let rows: Rows<Source> =
        serde_json::from_slice(&original.read("source-admission.json")).unwrap();
    assert_eq!(rows.rows.len(), 1222);
    assert_eq!(json!(rows.rows.len()), receipt["source_rows"]);
    let mut actual_rows = Vec::new();
    let mut differences = 0;
    for row in rows.rows {
        let actual = check_bytes(&hex(&row.source_hex));
        let capability = actual.as_ref().is_err_and(|e| e.phase == Phase::Capability);
        let matched = !capability && actual.is_ok() == row.accepted;
        differences += usize::from(!matched);
        actual_rows.push(
            json!({"source_hex":row.source_hex,"accepted":actual.is_ok(),
            "original_accepted":row.accepted,"capability":capability,"matched":matched,
            "diagnostic":actual.err().map(|e|e.to_string())}),
        );
    }
    save_json(
        "source-admission.actual.json",
        &json!({"rows":actual_rows,"differences":differences}),
    );
    assert_eq!(differences, 0);
}
#[test]
fn strict_gb_regression_boundaries() {
    for codec in CODECS {
        for raw in ["80", "8040", "ff", "81", "8130", "813081"] {
            let error = decode_bytes(&hex(raw), codec).unwrap_err();
            assert_eq!(error.phase, Phase::Decode, "{codec}: {raw}");
        }
        assert_eq!(decode_bytes(b"\x00A\r\n", codec).unwrap(), "\0A\r\n");
    }
    for (codec, raw, text) in [
        ("gb2312", "a1a4", "\u{30fb}"),
        ("gb2312", "a1aa", "\u{2015}"),
        ("gbk", "a1a4", "\u{00b7}"),
        ("gbk", "a1aa", "\u{2014}"),
        ("gbk", "a844", "\u{2015}"),
        ("cp936", "a1a4", "\u{00b7}"),
        ("euc_cn", "a1a4", "\u{30fb}"),
        ("gb18030", "81308130", "\u{80}"),
        ("gb18030", "8431a439", "\u{ffff}"),
        ("gb18030", "90308130", "\u{10000}"),
        ("gb18030_2000", "e3329a35", "\u{10ffff}"),
    ] {
        assert_eq!(
            decode_bytes(&hex(raw), codec).unwrap(),
            text,
            "{codec}: {raw}"
        );
    }
    for raw in [
        "8431a530", "85308130", "8f39fe39", "e3329a36", "fe39fe39", "8130812f",
    ] {
        assert_eq!(
            decode_bytes(&hex(raw), "gb18030").unwrap_err().phase,
            Phase::Decode
        );
    }
    assert!(decode_bytes(&hex("a844"), "gb2312").is_err());
    for codec in CODECS {
        // A valid prefix must never convert a strict suffix error into success.
        for suffix in ["80", "ff", "81", "8130", "813081"] {
            let mut raw = hex("d6d0");
            raw.extend_from_slice(&hex(suffix));
            assert_eq!(decode_bytes(&raw, codec).unwrap_err().phase, Phase::Decode);
        }
    }
}
