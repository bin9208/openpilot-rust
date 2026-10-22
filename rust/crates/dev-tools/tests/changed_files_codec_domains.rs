#![cfg(all(target_os = "linux", feature = "changed-files-native"))]
use openpilot_dev_tools::changed_files_check::{check_bytes, decode_bytes, named_character, Phase};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::PathBuf};
// Replaced only after the independent original capture artifact is authenticated.
const MANIFEST_SHA: &str = "f7affa43ae6f935688f3ee164103564df21066280ac637e510a6ec0deb7926ca";
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
fn fixture(name: &str) -> Vec<u8> {
    let root = PathBuf::from(
        std::env::var_os("AST_CODEC_ORIGINAL_DIR")
            .expect("authenticated independent original domain directory"),
    );
    let manifest = fs::read(root.join("manifest.json")).unwrap();
    assert_eq!(
        digest(&manifest),
        MANIFEST_SHA,
        "immutable independent original manifest"
    );
    let manifest: Manifest = serde_json::from_slice(&manifest).unwrap();
    let pin = manifest
        .files
        .get(name)
        .expect("fixture must be original-sealed");
    let raw = fs::read(root.join(name)).unwrap();
    assert_eq!(raw.len() as u64, pin.bytes);
    assert_eq!(digest(&raw), pin.sha256, "immutable original domain {name}");
    raw
}
fn save(name: &str, raw: &[u8]) {
    let root =
        PathBuf::from(std::env::var_os("AST_CODEC_PROOF").expect("actual native output directory"));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join(name), raw).unwrap();
}
fn save_json(name: &str, value: &Value) {
    save(name, &serde_json::to_vec(value).unwrap());
}
fn hex(raw: &str) -> Vec<u8> {
    assert_eq!(raw.len() % 2, 0);
    raw.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
fn hex_bytes(raw: &[u8]) -> String {
    raw.iter().map(|b| format!("{b:02x}")).collect()
}
#[derive(Deserialize)]
struct OriginalPayload {
    accepted: bool,
    #[serde(default)]
    utf8_hex: Option<String>,
}
fn compare(raw: &[u8], codec: &str, expected: &OriginalPayload) -> (Value, bool) {
    let actual = decode_bytes(raw, codec);
    let capability = actual.as_ref().is_err_and(|e| e.phase == Phase::Capability);
    let utf8_hex = actual.as_ref().ok().map(|v| hex_bytes(v.as_bytes()));
    let matches =
        !capability && actual.is_ok() == expected.accepted && utf8_hex == expected.utf8_hex;
    (
        json!({"source_hex":hex_bytes(raw),"native_accepted":actual.is_ok(),"native_utf8_hex":utf8_hex,"capability_error":capability,"native_diagnostic":actual.err().map(|e|e.to_string())}),
        matches,
    )
}
#[derive(Deserialize)]
struct Single {
    rows: BTreeMap<String, Vec<OriginalPayload>>,
}
#[derive(Deserialize)]
struct Two {
    codec: String,
    rows: Vec<OriginalPayload>,
}
#[derive(Deserialize)]
struct Vector {
    codec: String,
    hex: String,
    actual: OriginalPayload,
}
#[derive(Deserialize)]
struct Vectors {
    rows: Vec<Vector>,
}
#[test]
fn complete_original_byte_domains_compare_exact_decoded_payloads() {
    let original: Single = serde_json::from_slice(&fixture("single-byte.json")).unwrap();
    assert_eq!(original.rows.len(), 10);
    let mut differences = 0usize;
    let mut rows = 0usize;
    for (codec, expected) in original.rows {
        assert_eq!(expected.len(), 256);
        let actual: Vec<Value> = expected
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let (row, matched) = compare(&[u8::try_from(i).unwrap()], &codec, e);
                differences += usize::from(!matched);
                rows += 1;
                row
            })
            .collect();
        save_json(
            &format!("single-{codec}.json"),
            &json!({"codec":codec,"rows":actual}),
        );
    }
    for name in ["shift_jis-two-byte.json", "cp932-two-byte.json"] {
        let original: Two = serde_json::from_slice(&fixture(name)).unwrap();
        assert_eq!(original.rows.len(), 65536);
        let actual: Vec<Value> = original
            .rows
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let code = u16::try_from(i).unwrap();
                let (row, matched) = compare(&code.to_be_bytes(), &original.codec, e);
                differences += usize::from(!matched);
                rows += 1;
                row
            })
            .collect();
        save_json(name, &json!({"codec":original.codec,"rows":actual}));
    }
    let original: Vectors = serde_json::from_slice(&fixture("escape-utf7-vectors.json")).unwrap();
    let actual: Vec<Value> = original
        .rows
        .iter()
        .map(|v| {
            let (mut row, matched) = compare(&hex(&v.hex), &v.codec, &v.actual);
            row["codec"] = json!(v.codec);
            differences += usize::from(!matched);
            rows += 1;
            row
        })
        .collect();
    save_json("escape-utf7-vectors.json", &json!({"rows":actual}));
    save_json(
        "payload-summary.json",
        &json!({"rows":rows,"differences":differences,"all_capability_errors_unclosed":true}),
    );
    assert_eq!(
        differences, 0,
        "original strict decoded-byte mismatches; full native outputs retained"
    );
}
#[test]
fn original_utf7_all_scalars_match_complete_original_utf8_bytes() {
    let input = fixture("utf7-all-scalars.bin");
    let expected = fixture("utf8-all-scalars.bin");
    let actual = decode_bytes(&input, "utf-7");
    save_json(
        "utf7-all-scalars.json",
        &json!({"accepted":actual.is_ok(),"capability_error":actual.as_ref().is_err_and(|e|e.phase==Phase::Capability),"diagnostic":actual.as_ref().err().map(|e|e.to_string()),"input_bytes":input.len(),"original_utf8_bytes":expected.len(),"original_utf8_sha256":digest(&expected),"native_utf8_sha256":actual.as_ref().ok().map(|v|digest(v.as_bytes()))}),
    );
    if let Ok(text) = &actual {
        save("utf7-all-scalars.actual.utf8", text.as_bytes());
    }
    assert_eq!(actual.unwrap().as_bytes(), expected.as_slice());
}
#[derive(Deserialize)]
struct RegistryRow {
    codec: String,
    source_hex: String,
    accepted: bool,
}
#[derive(Deserialize)]
struct Registry {
    rows: Vec<RegistryRow>,
}
#[test]
fn every_original_standard_registry_cookie_is_closed_or_fails_explicitly() {
    let original: Registry =
        serde_json::from_slice(&fixture("standard-codec-admission.json")).unwrap();
    assert_eq!(original.rows.len(), 460);
    let mut differences = 0usize;
    let actual: Vec<Value>=original.rows.iter().map(|row|{
        let actual=check_bytes(&hex(&row.source_hex));
        let capability=actual.as_ref().is_err_and(|e|e.phase==Phase::Capability);
        let matched=!capability && actual.is_ok()==row.accepted;
        differences+=usize::from(!matched);
        json!({"codec":row.codec,"source_hex":row.source_hex,"original_accepted":row.accepted,"native_accepted":actual.is_ok(),"capability_error":capability,"native_diagnostic":actual.err().map(|e|e.to_string())})
    }).collect();
    save_json(
        "standard-codec-admission.json",
        &json!({"rows":actual,"differences":differences}),
    );
    assert_eq!(
        differences, 0,
        "valid unsupported text codecs remain unclosed, never counted as Python rejection"
    );
}
type AliasRow = (u32, String, String, Option<Vec<u32>>, Option<Vec<u32>>);
#[derive(Deserialize)]
struct Names {
    canonical: Vec<(u32, String, bool)>,
    aliases: Vec<AliasRow>,
    controls: Vec<(String, Option<Vec<u32>>)>,
}
fn named_actual(name: &str) -> (Option<Vec<u32>>, bool) {
    let point = named_character(name).ok().map(|c| vec![u32::from(c)]);
    let source = format!("x=\"\\N{{{name}}}\"\n");
    (point, check_bytes(source.as_bytes()).is_ok())
}
#[test]
fn all_original_unicode15_names_and_alias_versions_use_actual_ast_admission() {
    let original: Names = serde_json::from_slice(&fixture("names.json")).unwrap();
    let mut differences = 0usize;
    let canonical: Vec<Value> = original
        .canonical
        .iter()
        .map(|(cp, name, expected)| {
            let (value, accepted) = named_actual(name);
            let matched = accepted == *expected && value == Some(vec![*cp]);
            differences += usize::from(!matched);
            json!({"codepoint":cp,"name":name,"native_value":value,"native_ast_accepted":accepted})
        })
        .collect();
    let aliases: Vec<Value>=original.aliases.iter().map(|(cp,name,kind,lookup,ast)|{
        let (value,accepted)=named_actual(name);
        let matched=value==*lookup && accepted==ast.is_some();
        differences+=usize::from(!matched);
        json!({"codepoint":cp,"name":name,"kind":kind,"native_value":value,"native_ast_accepted":accepted})
    }).collect();
    let controls: Vec<Value> = original
        .controls
        .iter()
        .map(|(name, ast)| {
            let (value, accepted) = named_actual(name);
            let matched = accepted == ast.is_some() && (!accepted || value == *ast);
            differences += usize::from(!matched);
            json!({"name":name,"native_value":value,"native_ast_accepted":accepted})
        })
        .collect();
    save_json(
        "names.json",
        &json!({"canonical":canonical,"aliases":aliases,"controls":controls,"differences":differences}),
    );
    assert_eq!(
        differences, 0,
        "full independent original name/alias-version differences retained"
    );
}
#[test]
fn decoded_payload_preserves_nul_line_endings_and_missing_eof() {
    let raw = b"x\0\r\ny\r";
    assert_eq!(decode_bytes(raw, "utf8").unwrap().as_bytes(), raw);
    assert_eq!(decode_bytes(b"x", "utf8").unwrap(), "x");
    assert_eq!(decode_bytes(b"\xef\xbb\xbfx", "utf-8-sig").unwrap(), "x");
    assert!(check_bytes(raw).is_err());
    assert!(check_bytes(b"x=1").is_ok());
}
