#![cfg(all(target_os = "linux", feature = "changed-files-native"))]
//! Exact CP949 observations captured independently; current GNU backend unchanged.
use openpilot_dev_tools::changed_files_check::{check_bytes, decode_bytes, Phase};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    os::unix::fs::{FileExt, MetadataExt},
    path::{Path, PathBuf},
};

const PLAN_SHA: &str = "7e1a2467623b6d0a65e557323f9799342b723ad3d36d750c4845db3ae5f8e4bd";
const REUSED_SHA: &str = "9bdc06a0713139c302df8babf73c031cfe26053bbe8f42bf235ec6e9d9e40d1c";
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
fn save(name: &str, value: &Value) {
    fs::write(
        output().join(name),
        serde_json::to_vec_pretty(value).unwrap(),
    )
    .unwrap();
}
fn output() -> PathBuf {
    PathBuf::from(std::env::var_os("CP949_NATIVE_OUTPUT").expect("native output"))
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
        let root = PathBuf::from(std::env::var_os(format!("{prefix}_DIR")).unwrap());
        let raw = fs::read(root.join("manifest.json")).unwrap();
        assert_eq!(
            digest(&raw),
            std::env::var(format!("{prefix}_MANIFEST_SHA256")).unwrap()
        );
        Self {
            root,
            manifest: serde_json::from_slice(&raw).unwrap(),
        }
    }
    fn read(&self, name: &str) -> Vec<u8> {
        assert!(Path::new(name).file_name().unwrap() == name && name != "." && name != "..");
        let pin = self
            .manifest
            .files
            .get(name)
            .expect("sealed original member");
        let raw = fs::read(self.root.join(name)).unwrap();
        assert_eq!(raw.len(), pin.bytes);
        assert_eq!(digest(&raw), pin.sha256);
        raw
    }
    fn json(&self, name: &str) -> Value {
        serde_json::from_slice(&self.read(name)).unwrap()
    }
}
#[derive(Deserialize)]
struct Domain {
    name: String,
    width: usize,
    prefix_hex: String,
    suffix_hex: String,
    count: usize,
    skipped_indices: Vec<usize>,
    captured: usize,
}
#[derive(Deserialize)]
struct Payload {
    hex: String,
    accepted: bool,
    utf8_hex: Option<String>,
}
#[derive(Deserialize)]
struct Source {
    source_hex: String,
    accepted: bool,
}
#[derive(Deserialize)]
struct Rows<T> {
    rows: Vec<T>,
}
fn payload_actual(row: &Payload) -> (Value, bool) {
    let actual = decode_bytes(&hex(&row.hex), "cp949");
    let strict = actual
        .as_ref()
        .err()
        .is_none_or(|e| e.phase == Phase::Decode);
    let utf8 = actual.as_ref().ok().map(|s| bytes_hex(s.as_bytes()));
    let matched = strict && actual.is_ok() == row.accepted && utf8 == row.utf8_hex;
    (
        json!({"hex":row.hex,"accepted":actual.is_ok(),"utf8_hex":utf8,
        "strict_decode_result":strict,"matched":matched,
        "diagnostic":actual.err().map(|e|e.to_string())}),
        matched,
    )
}
fn mapping_paths(label: &str) -> BTreeSet<PathBuf> {
    let text = fs::read_to_string("/proc/self/maps").unwrap();
    fs::write(output().join(format!("maps.{label}.raw.txt")), &text).unwrap();
    let mut paths = BTreeSet::new();
    for line in text.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        assert!(fields.len() >= 5, "unparsed mapping: {line}");
        if fields.get(5).is_some_and(|p| p.starts_with('/')) {
            assert_eq!(fields.len(), 6, "escaped/deleted mapping: {line}");
            assert!(!fields[5].contains('\\'));
            let path = fs::canonicalize(fields[5]).unwrap();
            let metadata = fs::metadata(&path).unwrap();
            assert_eq!(metadata.ino(), fields[4].parse::<u64>().unwrap());
            assert!(metadata.is_file());
            paths.insert(path);
        }
    }
    assert!(paths.iter().any(|p| p.file_name().unwrap() == "libc.so.6"));
    // GNU glibc's CP949 converter is UHC. Missing actual mapping fails closed.
    assert!(paths.iter().any(|p| p.file_name().unwrap() == "UHC.so"));
    assert!(paths.contains(&fs::canonicalize(std::env::current_exe().unwrap()).unwrap()));
    paths
}
fn metadata(value: &fs::Metadata) -> Value {
    json!({"dev":value.dev(),"ino":value.ino(),"mode":value.mode(),"bytes":value.len(),
        "mtime":value.mtime(),"mtime_nsec":value.mtime_nsec(),
        "ctime":value.ctime(),"ctime_nsec":value.ctime_nsec()})
}
fn snapshot(files: &BTreeMap<PathBuf, File>) -> Value {
    let mut result = BTreeMap::new();
    for (path, file) in files {
        let first = file.metadata().unwrap();
        assert_eq!(metadata(&first), metadata(&fs::metadata(path).unwrap()));
        let mut h = Sha256::new();
        let mut at = 0;
        let mut buffer = [0u8; 65536];
        loop {
            let n = file.read_at(&mut buffer, at).unwrap();
            if n == 0 {
                break;
            }
            h.update(&buffer[..n]);
            at += u64::try_from(n).unwrap();
        }
        assert_eq!(at, first.len());
        assert_eq!(metadata(&first), metadata(&file.metadata().unwrap()));
        assert_eq!(metadata(&first), metadata(&fs::metadata(path).unwrap()));
        result.insert(
            path.to_string_lossy().into_owned(),
            json!({"stat":metadata(&first),
            "sha256":format!("{:x}",h.finalize()),"held_path_identity_verified":true}),
        );
    }
    json!(result)
}

#[test]
fn cp949_independent_domain_current_gnu_backend() {
    fs::create_dir(output()).unwrap();
    let original = Original::open("CP949_ORIGINAL");
    let supplement = Original::open("CP949_SUPPLEMENT");
    let receipt = original.json("receipt.json");
    assert_eq!(receipt["schema"], "cp949-original-domain-v1");
    assert_eq!(receipt["passed"], true);
    for flag in [
        "native_candidate_execution",
        "native_tables_used_as_expected",
        "prior_inputs_recaptured",
    ] {
        assert_eq!(receipt[flag], false);
    }
    for flag in [
        "actual_kr_and_multibyte_modules_mapped",
        "loaded_dsos_held",
        "held_seals_equal",
    ] {
        assert_eq!(receipt[flag], true);
    }
    assert_eq!(receipt["plan_sha256"], PLAN_SHA);
    let raw = original.read("input-plan.json");
    assert_eq!(digest(&raw), PLAN_SHA);
    let plan: Value = serde_json::from_slice(&raw).unwrap();
    let raw = original.read("reused-cp949.json");
    assert_eq!(digest(&raw), REUSED_SHA);
    let prior: Rows<Payload> = serde_json::from_slice(&raw).unwrap();
    let sup = supplement.json("receipt.json");
    assert_eq!(sup["schema"], "openpilot.codec-provider-completion/v1");
    assert_eq!(sup["passed"], true);
    assert_eq!(sup["rows"], 540);
    assert_eq!(sup["counts"]["cp949"], 45);
    assert_eq!(sup["canonical_byte_agreement"], true);
    assert_eq!(sup["held_seals_equal"], true);
    assert_eq!(sup["difference_rows"], 0);
    assert_eq!(sup["unexpected_errors"], 0);
    assert_eq!(sup["native_candidate_execution"], false);
    let canonical = supplement.read("new-selected.canonical.jsonl");
    assert_eq!(
        digest(&canonical),
        sup["new_canonical_sha256"].as_str().unwrap()
    );
    let selected: Vec<Value> = std::str::from_utf8(&canonical)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|row| row[0] == "cp949")
        .collect();
    assert_eq!(selected.len(), prior.rows.len());
    for (row, expected) in prior.rows.iter().zip(&selected) {
        assert_eq!(
            json!(["cp949", row.hex, row.accepted, row.utf8_hex]),
            *expected
        );
    }
    // One recorded new vector initializes GNU module dispatch before held seals.
    let warm = decode_bytes(b"provider-seal", "cp949");
    save(
        "dispatch-initialization.json",
        &json!({"hex":bytes_hex(b"provider-seal"),
        "accepted":warm.is_ok(),"utf8_hex":warm.as_ref().ok().map(|s|bytes_hex(s.as_bytes())),
        "diagnostic":warm.err().map(|e|e.to_string())}),
    );
    let paths = mapping_paths("before");
    let files: BTreeMap<_, _> = paths
        .iter()
        .map(|p| (p.clone(), File::open(p).unwrap()))
        .collect();
    let before = snapshot(&files);
    save("native-providers.before.json", &before);
    let mut seen: BTreeSet<_> = prior.rows.iter().map(|r| r.hex.clone()).collect();
    assert_eq!(seen.len(), 45);
    let mut differences = 0usize;
    let mut bad_phase = 0usize;
    // These45 accepted inputs are authenticated above and reused transitively.
    // Do not reexecute them merely because this new gate uses another runner.
    let prior_reused: Vec<_> = prior
        .rows
        .iter()
        .map(|row| {
            json!({"hex":row.hex,"accepted":row.accepted,
            "utf8_hex":row.utf8_hex,"native_evaluated":false})
        })
        .collect();
    save(
        "prior.reused.json",
        &json!({"rows":prior_reused,"new_native_calls":0}),
    );
    let descriptors = [
        (0, "", ""),
        (1, "", ""),
        (2, "", ""),
        (2, "41", ""),
        (2, "", "41"),
        (2, "", "81"),
    ];
    let domains: Vec<Domain> = serde_json::from_value(plan["domains"].clone()).unwrap();
    assert_eq!(domains.len(), descriptors.len());
    let mut total = 0usize;
    let mut first = Vec::new();
    for (ordinal, (domain, descriptor)) in domains.iter().zip(descriptors).enumerate() {
        assert_eq!(
            (
                domain.width,
                domain.prefix_hex.as_str(),
                domain.suffix_hex.as_str()
            ),
            descriptor
        );
        assert_eq!(domain.name, format!("domain-{ordinal:02}.bin"));
        assert_eq!(
            domain.count,
            256usize.pow(u32::try_from(domain.width).unwrap())
        );
        let expected = original.read(&domain.name);
        assert_eq!(expected.len(), domain.captured * 10);
        let mut actuals = Vec::new();
        let mut skipped = Vec::new();
        for index in 0..domain.count {
            let mut input = hex(&domain.prefix_hex);
            input.extend_from_slice(
                &u16::try_from(index).unwrap().to_be_bytes()[2 - domain.width..],
            );
            input.extend_from_slice(&hex(&domain.suffix_hex));
            if !seen.insert(bytes_hex(&input)) {
                skipped.push(index);
                continue;
            }
            let actual = decode_bytes(&input, "cp949");
            let mut record = [0u8; 10];
            if let Ok(text) = &actual {
                assert!(text.len() <= 8);
                record[0] = 1;
                record[1] = u8::try_from(text.len()).unwrap();
                record[2..2 + text.len()].copy_from_slice(text.as_bytes());
            } else if actual.as_ref().err().unwrap().phase != Phase::Decode {
                record[0] = 2;
                bad_phase += 1;
            }
            let at = actuals.len();
            if record.as_slice() != &expected[at..at + 10] {
                differences += 1;
                if first.len() < 100 {
                    first.push(json!({"domain":domain.name,"index":index,
                    "hex":bytes_hex(&input),"expected_record_hex":bytes_hex(&expected[at..at+10]),
                    "actual_record_hex":bytes_hex(&record),"diagnostic":actual.err().map(|e|e.to_string())}));
                }
            }
            actuals.extend_from_slice(&record);
            total += 1;
        }
        assert_eq!(skipped, domain.skipped_indices);
        assert_eq!(actuals.len(), expected.len());
        fs::write(output().join(&domain.name), actuals).unwrap();
    }
    let vectors: Rows<Payload> = serde_json::from_slice(&original.read("vectors.json")).unwrap();
    let mut actual_vectors = Vec::new();
    for row in &vectors.rows {
        assert!(seen.insert(row.hex.clone()));
        let (actual, matched) = payload_actual(row);
        differences += usize::from(!matched);
        actual_vectors.push(actual);
    }
    save("vectors.actual.json", &json!({"rows":actual_vectors}));
    let sources: Rows<Source> =
        serde_json::from_slice(&original.read("source-admission.json")).unwrap();
    let mut actual_sources = Vec::new();
    for row in &sources.rows {
        let actual = check_bytes(&hex(&row.source_hex));
        let phase_ok = actual
            .as_ref()
            .err()
            .is_none_or(|e| matches!(e.phase, Phase::Decode | Phase::Syntax));
        let matched = phase_ok && actual.is_ok() == row.accepted;
        differences += usize::from(!matched);
        actual_sources.push(
            json!({"source_hex":row.source_hex,"accepted":actual.is_ok(),
            "matched":matched,"phase_ok":phase_ok,"diagnostic":actual.err().map(|e|e.to_string())}),
        );
    }
    save("source.actual.json", &json!({"rows":actual_sources}));
    let after_paths = mapping_paths("after");
    let after = snapshot(&files);
    save("native-providers.after.json", &after);
    save(
        "summary.json",
        &json!({"dense_rows":total,"new_vectors":vectors.rows.len(),
        "source_rows":sources.rows.len(),"supplement_rows":prior.rows.len(),"differences":differences,
        "prior_rows_executed":0,
        "wrong_error_phase":bad_phase,"first_differences":first,"backend":"current-gnu-iconv-cp949",
        "provider_paths_equal":paths==after_paths,"provider_seals_equal":before==after}),
    );
    assert_eq!(total, 261870);
    assert_eq!(vectors.rows.len(), 4517);
    assert_eq!(sources.rows.len(), 880);
    assert_eq!(paths, after_paths);
    assert_eq!(before, after);
    assert_eq!(bad_phase, 0);
    assert_eq!(
        differences, 0,
        "actual CP949 differences retained; no implementation/tolerance change"
    );
}
