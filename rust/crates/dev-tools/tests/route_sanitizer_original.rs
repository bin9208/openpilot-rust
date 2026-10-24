//! a134b32c5의 실제 원본 capture를 사용하는 native source-absence 검증.
//! 원본 실행 없이 원문 bytes·source/fixture 전후 불변성을 확인한다.
#![cfg(all(target_os = "linux", feature = "route-sanitizer-native"))]

use capnp::{
    introspect::{Introspect, TypeVariant},
    message::{Builder, HeapAllocator, ReaderOptions},
    serialize::{read_message, write_message_to_words},
};
use openpilot_cereal::log_capnp::event;
use openpilot_dev_tools::route_sanitizer::{
    sanitize, sanitize_msg, sanitize_vin, LogMessage, PRESERVE_SERVICES,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    cell::Cell,
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{ErrorKind, Read},
    ops::Deref,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

const RETIRED_SOURCE: &str = "openpilot/tools/lib/sanitizer.py";
const RETIRED_SOURCE_SHA256: &str =
    "a931ad064af85e51567a909294738754a863bf31cbec4e7290fb5b0a383822d0";
const CAPTURE_HARNESS: &str = "rust/crates/dev-tools/tests/route_sanitizer_original.rs";
const CAPTURE_HARNESS_SHA256: &str =
    "06c3b91c2f51a84a9934270260386b5b4f76f798fbebaaaa40f3235c0d93c6ef";
const FIXTURE_DIR: &str = "rust/crates/dev-tools/tests/fixtures/route-sanitizer-original";
const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;

// 실제 CI의 원문 JSON bytes를 고정한다. 누락·변조·재직렬화는 거부한다.
const FROZEN_GROUPS: &[(&str, &str)] = &[
    (
        "vin-codepoints",
        "f4762d30d706a0956ab44b16c6b537cdd50092083ab6a8911e40c88b07ceba67",
    ),
    (
        "car-copy-float-bits",
        "7ff3b5e2ca762aa2c10f0fcf6c0e915f286143e51cde7ce2e1898ec1a1cb8958",
    ),
    (
        "non-car-identity",
        "8f10d1794cb35f382b807fc2f779d2eae181d7ec0966730852647d9ac1b449ac",
    ),
    (
        "filter-order-laziness",
        "8914183a7d663e5d3dd066c15450e041623d2f0e8cb5dfa70111f34c50a1eda7",
    ),
    (
        "malformed-boundaries",
        "1553bbccfaf9d20074eac1948a1f4038c31277534bdace844a77e6269d9d2924",
    ),
];

// 현재 capture의 input.source_paths 원문 순서. 비교할 authored payload와 명시 분리한다.
const CAPTURE_SOURCE_PATHS: &[&str] = &[
    "openpilot/__init__.py",
    "openpilot/tools/__init__.py",
    "openpilot/tools/lib/__init__.py",
    "openpilot/tools/lib/sanitizer.py",
    "openpilot/cereal/__init__.py",
    "openpilot/cereal/log.capnp",
    "openpilot/cereal/custom.capnp",
    "openpilot/cereal/deprecated.capnp",
    "openpilot/cereal/include/c++.capnp",
    "opendbc_repo/opendbc/car/car.capnp",
    "opendbc_repo/opendbc/car/include/c++.capnp",
    "rust/crates/dev-tools/src/route_sanitizer.rs",
    "rust/crates/dev-tools/src/route_sanitizer_tests.rs",
    "rust/crates/dev-tools/tests/route_sanitizer_original.rs",
];
const CURRENT_SOURCE_PATHS: &[&str] = &[
    "openpilot/cereal/log.capnp",
    "openpilot/cereal/custom.capnp",
    "openpilot/cereal/deprecated.capnp",
    "openpilot/cereal/include/c++.capnp",
    "opendbc_repo/opendbc/car/car.capnp",
    "opendbc_repo/opendbc/car/include/c++.capnp",
    "rust/crates/dev-tools/src/route_sanitizer.rs",
    "rust/crates/dev-tools/src/route_sanitizer_tests.rs",
];
// 현재 실행 dependency로 재사용하지 않는 역사 source 기록. 삭제 대상은 RETIRED_SOURCE 하나다.
const HISTORICAL_SOURCE_PATHS: &[&str] = &[
    "openpilot/__init__.py",
    "openpilot/tools/__init__.py",
    "openpilot/tools/lib/__init__.py",
    "openpilot/tools/lib/sanitizer.py",
    "openpilot/cereal/__init__.py",
    "rust/crates/dev-tools/tests/route_sanitizer_original.rs",
];

type Identity = (u64, u64, u32, u32, u32, u64, i64, i64, i64, i64);
#[derive(Debug, PartialEq, Eq)]
struct Seal {
    identity: Identity,
    sha256: String,
}
fn identity(m: &fs::Metadata) -> Identity {
    (
        m.dev(),
        m.ino(),
        m.mode(),
        m.uid(),
        m.gid(),
        m.len(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec(),
    )
}
fn is_sha256(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn read_sealed(path: &Path) -> Result<(Vec<u8>, Seal), String> {
    let error = |e| format!("{}: {e}", path.display());
    let named = fs::symlink_metadata(path).map_err(error)?;
    if !named.is_file() || named.file_type().is_symlink() || named.len() > MAX_FILE_BYTES {
        return Err(format!("bounded regular file required: {}", path.display()));
    }
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(path)
        .map_err(error)?;
    let before = file.metadata().map_err(error)?;
    if identity(&before) != identity(&named) {
        return Err(format!("file changed before read: {}", path.display()));
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(error)?;
    let after = file.metadata().map_err(error)?;
    let named_after = fs::symlink_metadata(path).map_err(error)?;
    if bytes.len() as u64 != before.len()
        || bytes.len() as u64 > MAX_FILE_BYTES
        || identity(&before) != identity(&after)
        || identity(&before) != identity(&named_after)
    {
        return Err(format!("file changed during read: {}", path.display()));
    }
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    Ok((
        bytes,
        Seal {
            identity: identity(&before),
            sha256,
        },
    ))
}
fn require_absent(root: &Path) -> Result<(), String> {
    let path = root.join(RETIRED_SOURCE);
    // broken symlink도 부재로 인정하지 않는다.
    if path.exists() || path.is_symlink() {
        return Err(format!(
            "retired source still exists or is a symlink: {}",
            path.display()
        ));
    }
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!(
            "cannot establish source absence: {}: {e}",
            path.display()
        )),
        Ok(_) => Err(format!("retired source appeared: {}", path.display())),
    }
}
fn object_keys(value: &Value, keys: &[&str]) {
    let object = value.as_object().expect("capture object required");
    assert_eq!(object.len(), keys.len(), "capture object key count changed");
    assert!(
        keys.iter().all(|key| object.contains_key(*key)),
        "capture keys changed"
    );
}
fn validate_capture(group: &str, capture: &Value) {
    object_keys(
        capture,
        &[
            "schema",
            "group",
            "input",
            "expected_original",
            "unknown_fields",
        ],
    );
    assert_eq!(capture["schema"], "openpilot.route-sanitizer-original.v1");
    assert_eq!(capture["group"], group);
    assert_eq!(capture["unknown_fields"], "기존 native unit의 raw assertion으로 별도 검증; 공통 canonical 비교의 증거에 포함하지 않음");
    let input = &capture["input"];
    assert_eq!(input["source_paths"], json!(CAPTURE_SOURCE_PATHS));
    let (mode, wires, rows) = match group {
        "vin-codepoints" => ("messages", 14, 14),
        "car-copy-float-bits" => ("messages", 8, 8),
        "non-car-identity" => ("messages", 4, 4),
        "filter-order-laziness" => ("stream", 9, 6),
        "malformed-boundaries" => ("boundaries", 5, 5),
        _ => panic!("unknown frozen group"),
    };
    object_keys(
        input,
        match mode {
            "messages" => &["mode", "wires", "vins", "source_paths"],
            "stream" => &["mode", "wires", "source_paths"],
            "boundaries" => &["mode", "wires", "good", "source_paths"],
            _ => unreachable!(),
        },
    );
    assert_eq!(input["mode"], mode);
    assert_eq!(input["wires"].as_array().unwrap().len(), wires);
    let result = &capture["expected_original"];
    object_keys(result, &["metadata", "roster", "vins", "observations"]);
    assert_eq!(result["roster"], json!(PRESERVE_SERVICES));
    assert_eq!(
        result["vins"].as_array().unwrap().len(),
        if group == "vin-codepoints" { 14 } else { 0 }
    );
    let observations = &result["observations"];
    if mode == "stream" {
        assert_eq!(observations["trace"], json!([0, 2, 3, 4, 6, 7, 8, 10]));
        assert_eq!(observations["rows"].as_array().unwrap().len(), rows);
    } else {
        assert_eq!(observations.as_array().unwrap().len(), rows);
    }
    let metadata = &result["metadata"];
    object_keys(metadata, &["python", "pycapnp", "sources", "providers"]);
    assert!(!metadata["python"].as_str().unwrap().is_empty());
    assert_eq!(metadata["pycapnp"], "2.1.0");
    let sources = metadata["sources"].as_object().unwrap();
    assert_eq!(sources.len(), CAPTURE_SOURCE_PATHS.len());
    for path in CAPTURE_SOURCE_PATHS {
        assert!(
            is_sha256(sources[*path].as_str().unwrap()),
            "invalid source hash: {path}"
        );
        assert!(
            CURRENT_SOURCE_PATHS.contains(path) ^ HISTORICAL_SOURCE_PATHS.contains(path),
            "unclassified source: {path}"
        );
    }
    assert_eq!(sources[RETIRED_SOURCE], RETIRED_SOURCE_SHA256);
    assert_eq!(sources[CAPTURE_HARNESS], CAPTURE_HARNESS_SHA256);
    // 역사 provider 경로/hash만 검증한다. 현재 파일을 열거나 실행하지 않는다.
    let providers = metadata["providers"].as_object().unwrap();
    assert_eq!(providers.len(), 3);
    for (path, digest) in providers {
        assert!(Path::new(path).is_absolute() && is_sha256(digest.as_str().unwrap()));
    }
}

struct FrozenObservation {
    result: Value,
    root: PathBuf,
    group: String,
    fixture_sha256: String,
    seals: BTreeMap<String, Seal>,
}
impl Deref for FrozenObservation {
    type Target = Value;
    fn deref(&self) -> &Self::Target {
        &self.result
    }
}
impl FrozenObservation {
    fn verify_after(&self) -> Result<(), String> {
        require_absent(&self.root)?;
        for (path, before) in &self.seals {
            let (_, after) = read_sealed(&self.root.join(path))?;
            if *before != after {
                return Err(format!(
                    "source or fixture changed during native assertions: {path}"
                ));
            }
        }
        require_absent(&self.root)
    }
}
impl Drop for FrozenObservation {
    fn drop(&mut self) {
        let result = self.verify_after();
        if std::thread::panicking() {
            if let Err(error) = result {
                eprintln!("frozen reference after guard also failed: {error}");
            }
            return;
        }
        result.expect("frozen reference after guard failed");
        // 원본 capture 접두사를 재사용하지 않는다. 본문과 after guard를 통과한 뒤만 출력한다.
        println!(
            "ROUTE_SANITIZER_FROZEN_CHECK {}",
            json!({
                "group": &self.group,
                "fixture_sha256": &self.fixture_sha256,
                "source_absent_before_and_after": true,
                "source_and_fixture_seals_unchanged": true,
                "providers_are_historical_metadata": true,
                "fresh_original_executions": 0,
            })
        );
    }
}

fn oracle(group: &str, request: Value) -> FrozenObservation {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    require_absent(&root).expect("original source must be absent before native comparison");
    assert!(
        FROZEN_GROUPS.iter().any(|(name, _)| *name == group),
        "unknown authored group"
    );
    assert_eq!(FROZEN_GROUPS.len(), 5);
    for &(name, pin) in FROZEN_GROUPS {
        assert!(
            is_sha256(pin),
            "successful CI capture/full fixture SHA missing for {name}"
        );
    }
    let mut seals = BTreeMap::new();
    let mut selected = None;
    let mut shared_metadata: Option<Value> = None;
    for &(name, pin) in FROZEN_GROUPS {
        let path = format!("{FIXTURE_DIR}/{name}.original.json");
        let (bytes, seal) = read_sealed(&root.join(&path)).expect("frozen capture missing/invalid");
        assert_eq!(
            seal.sha256.as_str(),
            pin,
            "full fixture SHA differs: {name}"
        );
        let capture: Value = serde_json::from_slice(&bytes).expect("invalid frozen capture JSON");
        validate_capture(name, &capture);
        let metadata = &capture["expected_original"]["metadata"];
        if let Some(shared) = &shared_metadata {
            assert_eq!(
                metadata, shared,
                "five captures must share exact source/provider metadata"
            );
        } else {
            shared_metadata = Some(metadata.clone());
        }
        if name == group {
            selected = Some((capture, seal.sha256.clone()));
        }
        assert!(seals.insert(path, seal).is_none());
    }
    let (capture, fixture_sha256) = selected.unwrap();
    let mut authored = capture["input"].as_object().unwrap().clone();
    let source_paths = authored
        .remove("source_paths")
        .expect("capture source_paths required");
    assert_eq!(source_paths, json!(CAPTURE_SOURCE_PATHS));
    // mode/wires/vins/good의 문자열 byte와 전체 배열/객체를 그대로 비교한다.
    // 이 한 metadata 필드 외에 제거·재구성·정규화하는 입력은 없다.
    assert_eq!(
        Value::Object(authored),
        request,
        "authored request differs from frozen input"
    );
    let result = capture["expected_original"].clone();
    for path in CURRENT_SOURCE_PATHS {
        let (_, seal) =
            read_sealed(&root.join(path)).expect("current native/schema source missing");
        assert_eq!(
            json!(&seal.sha256),
            result["metadata"]["sources"][*path],
            "current native/schema source differs: {path}"
        );
        assert!(seals.insert((*path).to_owned(), seal).is_none());
    }
    require_absent(&root).expect("original source appeared during frozen admission");
    FrozenObservation {
        result,
        root,
        group: group.to_owned(),
        fixture_sha256,
        seals,
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn unhex(value: &Value) -> Vec<u8> {
    let text = value.as_str().unwrap();
    assert_eq!(text.len() % 2, 0);
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}
fn fixture(service: &str, vin: &str, bits: u32, time: u64) -> Builder<HeapAllocator> {
    let mut message = Builder::new_default();
    let mut root = message.init_root::<event::Builder<'_>>();
    root.set_log_mono_time(time);
    root.set_valid(false);
    match service {
        "carParams" => {
            let mut cp = root.init_car_params();
            cp.set_car_vin(vin);
            cp.set_brand("synthetic-brand-甲");
            cp.set_car_fingerprint("SYNTHETIC_FIXTURE_ONLY");
            cp.set_flags(0x5a5a_0101);
            cp.set_mass(f32::from_bits(bits));
            cp.set_wheelbase(f32::from_bits(0x8000_0000));
            let mut points = cp.reborrow().init_longitudinal_tuning().init_kp_b_p(3);
            for (i, bits) in [0x3fa0_0000, 1, 0x7fc1_2345].into_iter().enumerate() {
                points.set(i as u32, f32::from_bits(bits));
            }
            let mut fw = cp.init_car_fw(1).get(0);
            fw.set_address(0x7e0);
            fw.set_fw_version(&[0, 0xff, 1, 0x80]);
        }
        "can" => {
            let mut can = root.init_can(1).get(0);
            can.set_address(0x123);
            can.set_src(0x82);
            can.set_dat(&[0, 0xff, 2, 0x80]);
        }
        "pandaStates" | "pandaStateDEPRECATED" => {
            let mut panda = if service == "pandaStates" {
                root.init_panda_states(1).get(0)
            } else {
                root.init_panda_state_d_e_p_r_e_c_a_t_e_d()
            };
            panda.set_uptime(0xf123_4567);
            panda.set_voltage(0x1020_3040);
            panda.set_ignition_line(true);
            panda.set_interrupt_load(f32::from_bits(bits));
        }
        "logMessage" => root.set_log_message("synthetic-filtered-甲"),
        _ => panic!("unauthored synthetic service"),
    }
    message
}
fn wire(reader: event::Reader<'_>) -> Vec<u8> {
    let mut builder = Builder::new_default();
    builder.set_root(reader).unwrap();
    write_message_to_words(&builder)
}
fn canonical(raw: &[u8]) -> Vec<u8> {
    // Untyped whole-wire canonicalization: no to_dict()/known-field rebuild or masks.
    let message = read_message(raw, ReaderOptions::new()).unwrap();
    capnp::Word::words_to_bytes(&message.canonicalize().unwrap()).to_vec()
}
fn fields(reader: event::Reader<'_>) -> Value {
    let mut result = json!({"time":reader.get_log_mono_time(), "valid":reader.get_valid()});
    match reader.which().unwrap() {
        event::CarParams(cp) => {
            let cp = cp.unwrap();
            result["service"] = json!("carParams");
            result["vin"] = json!(cp.get_car_vin().unwrap().to_str().unwrap());
            result["brand"] = json!(cp.get_brand().unwrap().to_str().unwrap());
            result["fingerprint"] = json!(cp.get_car_fingerprint().unwrap().to_str().unwrap());
            result["flags"] = json!(cp.get_flags());
            result["mass_bits"] = json!(cp.get_mass().to_bits());
            result["wheelbase_bits"] = json!(cp.get_wheelbase().to_bits());
            result["kp_bits"] = json!(cp
                .get_longitudinal_tuning()
                .unwrap()
                .get_kp_b_p()
                .unwrap()
                .iter()
                .map(f32::to_bits)
                .collect::<Vec<_>>());
            result["fw"] = json!(cp.get_car_fw().unwrap().iter().map(|fw| json!({"address":fw.get_address(), "data":hex(fw.get_fw_version().unwrap())})).collect::<Vec<_>>());
        }
        event::Can(can) => {
            result["service"] = json!("can");
            result["can"] = json!(can.unwrap().iter().map(|c| json!({"address":c.get_address(), "src":c.get_src(), "data":hex(c.get_dat().unwrap())})).collect::<Vec<_>>());
        }
        event::PandaStates(pandas) => {
            result["service"] = json!("pandaStates");
            result["pandas"] = json!(pandas.unwrap().iter().map(panda_fields).collect::<Vec<_>>());
        }
        event::PandaStateDEPRECATED(panda) => {
            result["service"] = json!("pandaStateDEPRECATED");
            result["pandas"] = json!([panda_fields(panda.unwrap())]);
        }
        event::LogMessage(text) => {
            result["service"] = json!("logMessage");
            result["text"] = json!(text.unwrap().to_str().unwrap());
        }
        _ => panic!("unauthored synthetic service"),
    }
    result
}
fn panda_fields(p: openpilot_cereal::log_capnp::panda_state::Reader<'_>) -> Value {
    json!({"uptime":p.get_uptime(), "voltage":p.get_voltage(), "ignition":p.get_ignition_line(), "load_bits":p.get_interrupt_load().to_bits()})
}
fn compare_messages(
    group: &str,
    inputs: &[Builder<HeapAllocator>],
    vins: &[&str],
) -> FrozenObservation {
    let wires = inputs
        .iter()
        .map(write_message_to_words)
        .collect::<Vec<_>>();
    let original = oracle(
        group,
        json!({"mode":"messages", "wires":wires.iter().map(|w| hex(w)).collect::<Vec<_>>(), "vins":vins}),
    );
    let rows = original["observations"].as_array().unwrap();
    assert_eq!(rows.len(), inputs.len());
    for ((input, raw), row) in inputs.iter().zip(&wires).zip(rows) {
        let reader = input.get_root_as_reader::<event::Reader<'_>>().unwrap();
        let before = fields(reader);
        let output = sanitize_msg(LogMessage::Borrowed(reader)).unwrap();
        let output_reader = output.as_reader().unwrap();
        let is_car = before["service"] == "carParams";
        assert_eq!(matches!(output, LogMessage::Borrowed(_)), !is_car);
        assert_eq!(row["same"], json!(!is_car));
        assert_eq!(row["before_wire"], row["after_wire"]);
        assert_eq!(row["before"], row["after"]);
        assert_eq!(before, row["before"]);
        assert_eq!(fields(output_reader), row["output"]);
        assert_eq!(
            canonical(&wire(output_reader)),
            canonical(&unhex(&row["output_wire"]))
        );
        assert_eq!(canonical(raw), canonical(&unhex(&row["after_wire"])));
        assert_eq!(write_message_to_words(input), *raw);
        assert_eq!(fields(reader), before);
        if let (event::CarParams(a), event::CarParams(b)) =
            (reader.which().unwrap(), output_reader.which().unwrap())
        {
            let (a, b) = (a.unwrap(), b.unwrap());
            assert_ne!(
                a.get_car_vin().unwrap().as_bytes().as_ptr(),
                b.get_car_vin().unwrap().as_bytes().as_ptr()
            );
            // VIN만 복원하면 입력의 전체 wire graph와 같아야 한다. 숫자/필드는 마스킹하지 않는다.
            let mut restored = Builder::new_default();
            restored.set_root(output_reader).unwrap();
            let event::CarParams(cp) = restored
                .get_root::<event::Builder<'_>>()
                .unwrap()
                .which()
                .unwrap()
            else {
                panic!("carParams copy changed union");
            };
            cp.unwrap()
                .set_car_vin(a.get_car_vin().unwrap().to_str().unwrap());
            assert_eq!(
                canonical(raw),
                canonical(&write_message_to_words(&restored))
            );
        }
        if let (event::Can(a), event::Can(b)) =
            (reader.which().unwrap(), output_reader.which().unwrap())
        {
            assert_eq!(
                a.unwrap().get(0).get_dat().unwrap().as_ptr(),
                b.unwrap().get(0).get_dat().unwrap().as_ptr()
            );
        }
    }
    original
}

#[test]
fn original_vin_utf8_codepoint_boundaries() {
    let vins = [
        "",
        "a",
        "abcde",
        "abcdef",
        "abcdefg",
        "prefixABCDEF",
        "前缀甲乙丙丁戊己",
        "é🦀甲乙丙丁戊己",
        "A\u{301}BCDEF",
        "a\0bcdef",
        "\0abcdef",
        "👩\u{200d}🔬甲乙丙丁戊己",
        "\u{10ffff}abcdef",
        "x\nabcde",
    ];
    let inputs = vins
        .iter()
        .enumerate()
        .map(|(i, vin)| fixture("carParams", vin, 0x449a_5000, i as u64 + 1))
        .collect::<Vec<_>>();
    let original = compare_messages("vin-codepoints", &inputs, &vins);
    for (vin, expected) in vins.iter().zip(original["vins"].as_array().unwrap()) {
        assert_eq!(json!(sanitize_vin(vin)), *expected);
    }
    assert_eq!(original["vins"].as_array().unwrap().len(), 14);
}

#[test]
fn original_car_params_copy_preserves_full_wire_and_float_bits() {
    let patterns = [
        0,
        0x8000_0000,
        1,
        0x007f_ffff,
        0x7f7f_ffff,
        0x7f80_0000,
        0xff80_0000,
        0x7fc1_2345,
    ];
    let inputs = patterns
        .iter()
        .enumerate()
        .map(|(i, bits)| {
            fixture(
                "carParams",
                "synthetic-prefixABCDEF",
                *bits,
                0x1234_5678_9abc_def0 + i as u64,
            )
        })
        .collect::<Vec<_>>();
    let original = compare_messages("car-copy-float-bits", &inputs, &[]);
    for (row, bits) in original["observations"]
        .as_array()
        .unwrap()
        .iter()
        .zip(patterns)
    {
        assert_eq!(row["output"]["mass_bits"], json!(bits));
        assert_eq!(row["output"]["wheelbase_bits"], json!(0x8000_0000u32));
        assert_eq!(
            row["output"]["kp_bits"],
            json!([0x3fa0_0000u32, 1, 0x7fc1_2345u32])
        );
        assert_eq!(row["output"]["brand"], "synthetic-brand-甲");
        assert_eq!(row["output"]["flags"], json!(0x5a5a_0101u32));
        assert_eq!(
            row["output"]["fw"],
            json!([{"address":0x7e0,"data":"00ff0180"}])
        );
    }
}

#[test]
fn original_non_car_messages_preserve_identity_and_fields() {
    let inputs = ["can", "pandaStates", "pandaStateDEPRECATED", "logMessage"]
        .iter()
        .enumerate()
        .map(|(i, service)| fixture(service, "", 0x8000_0000, 100 + i as u64))
        .collect::<Vec<_>>();
    compare_messages("non-car-identity", &inputs, &[]);
}

struct Observed<'a> {
    messages: std::vec::IntoIter<LogMessage<'a>>,
    iterated: &'a Cell<usize>,
    pulls: &'a Cell<usize>,
}
struct Pulls<'a>(std::vec::IntoIter<LogMessage<'a>>, &'a Cell<usize>);
impl<'a> IntoIterator for Observed<'a> {
    type Item = capnp::Result<LogMessage<'a>>;
    type IntoIter = Pulls<'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.iterated.set(self.iterated.get() + 1);
        Pulls(self.messages, self.pulls)
    }
}
impl<'a> Iterator for Pulls<'a> {
    type Item = capnp::Result<LogMessage<'a>>;
    fn next(&mut self) -> Option<Self::Item> {
        self.1.set(self.1.get() + 1);
        self.0.next().map(Ok)
    }
}
#[test]
fn original_service_filter_order_and_laziness() {
    let services = [
        "logMessage",
        "pandaStates",
        "can",
        "carParams",
        "logMessage",
        "pandaStateDEPRECATED",
        "carParams",
        "can",
        "logMessage",
    ];
    let inputs = services
        .iter()
        .enumerate()
        .map(|(i, name)| fixture(name, "prefixABCDEF", 0x3fa0_0000, i as u64 + 100))
        .collect::<Vec<_>>();
    let wires = inputs
        .iter()
        .map(write_message_to_words)
        .collect::<Vec<_>>();
    let original = oracle(
        "filter-order-laziness",
        json!({"mode":"stream", "wires":wires.iter().map(|w| hex(w)).collect::<Vec<_>>()}),
    );
    let observed = &original["observations"];
    let (iterated, pulls) = (Cell::new(0), Cell::new(0));
    let source = Observed {
        messages: inputs
            .iter()
            .map(|m| LogMessage::Borrowed(m.get_root_as_reader().unwrap()))
            .collect::<Vec<_>>()
            .into_iter(),
        iterated: &iterated,
        pulls: &pulls,
    };
    let mut output = sanitize(source);
    let mut trace = vec![pulls.get()];
    assert_eq!(iterated.get(), 1);
    assert_eq!(observed["iterated"], 1);
    let rows = observed["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 6);
    for row in rows {
        let output = output.next().unwrap().unwrap();
        assert_eq!(
            matches!(output, LogMessage::Borrowed(_)),
            row["same"].as_bool().unwrap()
        );
        assert_eq!(fields(output.as_reader().unwrap()), row["fields"]);
        assert_eq!(
            canonical(&wire(output.as_reader().unwrap())),
            canonical(&unhex(&row["wire"]))
        );
        trace.push(pulls.get());
    }
    assert!(output.next().is_none());
    trace.push(pulls.get());
    assert_eq!(json!(iterated.get()), observed["final_iterated"]);
    assert_eq!(json!(trace), observed["trace"]);
    assert_eq!(trace, [0, 2, 3, 4, 6, 7, 8, 10]);
    for (raw, after) in wires
        .iter()
        .zip(observed["inputs_after"].as_array().unwrap())
    {
        assert_eq!(canonical(raw), canonical(&unhex(after)));
    }
}

fn words(raw: &[u8]) -> Vec<u64> {
    assert_eq!(
        &raw[..4],
        &[0; 4],
        "single segment authored fixture required"
    );
    raw[8..]
        .chunks_exact(8)
        .map(|w| u64::from_le_bytes(w.try_into().unwrap()))
        .collect()
}
fn target(data: &[u64], pointer: usize) -> usize {
    usize::try_from(pointer as i64 + 1 + i64::from((data[pointer] as u32 as i32) >> 2)).unwrap()
}
fn framed(data: &[u64]) -> Vec<u8> {
    let mut raw = vec![0; 4];
    raw.extend_from_slice(&u32::try_from(data.len()).unwrap().to_le_bytes());
    for word in data {
        raw.extend_from_slice(&word.to_le_bytes());
    }
    raw
}
#[test]
fn original_malformed_boundaries_are_lazy_and_leave_inputs_unchanged() {
    // 기존 unit fixture와 같은 invalid UTF-8 / unknown tag / wrong pointer kind.
    let good = write_message_to_words(&fixture("can", "", 0, 900));
    let mut invalid_vin =
        write_message_to_words(&fixture("carParams", "synthetic-utf8-marker", 0, 901));
    let offset = invalid_vin
        .windows(b"synthetic-utf8-marker".len())
        .position(|w| w == b"synthetic-utf8-marker")
        .unwrap();
    invalid_vin[offset] = 0xff;
    let mut unknown = words(&good);
    let TypeVariant::Struct(schema) = event::Owned::introspect().which() else {
        panic!("Event schema");
    };
    let capnp::schema_capnp::node::Struct(proto) = capnp::schema::StructSchema::new(schema)
        .get_proto()
        .which()
        .unwrap()
    else {
        panic!("Event struct");
    };
    let discriminator = proto.get_discriminant_offset() as usize;
    let root = target(&unknown, 0);
    let shift = (discriminator % 4) * 16;
    unknown[root + discriminator / 4] =
        (unknown[root + discriminator / 4] & !(0xffff << shift)) | (0xffff << shift);
    let wrong_payload = |service| {
        let mut data = words(&write_message_to_words(&fixture(service, "", 0, 902)));
        let root = target(&data, 0);
        let pointer = root + ((data[0] >> 32) & 0xffff) as usize;
        let offset = i32::try_from(root as i64 - pointer as i64 - 1).unwrap();
        data[pointer] = (1 << 32) | u64::from((offset as u32) << 2);
        framed(&data)
    };
    let mut truncated = good.clone();
    truncated.pop();
    let cases = [
        invalid_vin,
        framed(&unknown),
        wrong_payload("can"),
        wrong_payload("logMessage"),
        truncated,
    ];
    let original = oracle(
        "malformed-boundaries",
        json!({"mode":"boundaries", "good":hex(&good), "wires":cases.iter().map(|w| hex(w)).collect::<Vec<_>>()}),
    );
    let rows = original["observations"].as_array().unwrap();
    assert_eq!(rows.len(), 5);
    for (index, (raw, row)) in cases.iter().zip(rows).enumerate() {
        let input = read_message(raw.as_slice(), ReaderOptions::new());
        if index == 4 {
            assert!(input.is_err());
            assert_eq!(row["decode_error"], true);
            continue;
        }
        assert_eq!(row["decode_error"], false, "{row}");
        assert_eq!(row["before_wire"], row["after_wire"]);
        assert_eq!(canonical(raw), canonical(&unhex(&row["after_wire"])));
        let input = input.unwrap();
        let good_input = read_message(good.as_slice(), ReaderOptions::new()).unwrap();
        let (iterated, pulls) = (Cell::new(0), Cell::new(0));
        let source = Observed {
            messages: vec![
                LogMessage::Borrowed(input.get_root().unwrap()),
                LogMessage::Borrowed(good_input.get_root().unwrap()),
            ]
            .into_iter(),
            iterated: &iterated,
            pulls: &pulls,
        };
        let mut output = sanitize(source);
        let mut trace = vec![pulls.get()];
        assert_eq!(row["iterated"], json!(iterated.get()));
        let steps = row["steps"].as_array().unwrap();
        assert_eq!(steps.len(), if index == 3 { 2 } else { 3 });
        let expected_kinds: &[&str] = match index {
            0 | 1 => &["error", "message", "end"],
            2 => &["message", "message", "end"],
            3 => &["message", "end"],
            _ => unreachable!(),
        };
        assert_eq!(
            steps
                .iter()
                .map(|step| step["kind"].as_str().unwrap())
                .collect::<Vec<_>>(),
            expected_kinds
        );
        if index == 2 {
            assert_eq!(steps[0]["payload_error"], true);
            assert_eq!(steps[1]["payload_error"], false);
        }
        for step in steps {
            match output.next() {
                None => assert_eq!(step["kind"], "end"),
                Some(Err(_)) => assert_eq!(step["kind"], "error"),
                Some(Ok(message)) => {
                    assert_eq!(step["kind"], "message");
                    assert_eq!(step["same"], true);
                    assert!(matches!(message, LogMessage::Borrowed(_)));
                    let reader = message.as_reader().unwrap();
                    assert_eq!(step["time"], json!(reader.get_log_mono_time()));
                    let event::Can(payload) = reader.which().unwrap() else {
                        panic!("CAN boundary expected");
                    };
                    assert_eq!(step["payload_error"], json!(payload.is_err()));
                }
            }
            trace.push(pulls.get());
        }
        assert_eq!(json!(trace), row["trace"]);
        assert_eq!(json!(iterated.get()), row["final_iterated"]);
        assert_eq!(
            trace,
            if index == 3 {
                vec![0, 2, 3]
            } else {
                vec![0, 1, 2, 3]
            }
        );
        assert_eq!(canonical(raw), canonical(&wire(input.get_root().unwrap())));
    }
}

#[test]
fn retired_source_reappearance_files_directories_and_broken_links_are_rejected() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let retired = root.join(RETIRED_SOURCE);
    fs::create_dir_all(retired.parent().unwrap()).unwrap();
    assert!(require_absent(root).is_ok());
    fs::write(&retired, b"synthetic reappearance control\n").unwrap();
    assert!(require_absent(root).is_err());
    fs::remove_file(&retired).unwrap();
    symlink("missing-synthetic-target", &retired).unwrap();
    assert!(!retired.exists());
    assert!(require_absent(root).is_err());
    fs::remove_file(&retired).unwrap();
    fs::create_dir(&retired).unwrap();
    assert!(require_absent(root).is_err());
    fs::remove_dir(&retired).unwrap();
    assert!(require_absent(root).is_ok());
}
