#![cfg(feature = "native-skip-miri")]

use openpilot_model_runtime::compiler_buffers::{
    descriptor, host_buffer, Buffer, BufferTable, Descriptor, OfflineBufferTable,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn buffer(base: u64, allocation_bytes: usize, offset: usize, bytes: usize) -> Buffer {
    Buffer {
        base,
        allocation_bytes,
        offset,
        bytes,
    }
}

#[test]
fn device_admission_keeps_single_buffer_and_exact_device_spelling() {
    assert!(host_buffer(Descriptor::NonSingle).is_err());
    assert!(descriptor(Descriptor::NonSingle).is_err());
    for device in [
        "CPU", "CPU:0", "CPU:LLVM", "NPY", "NPY:2", "PYTHON", "QCOM", "QCOM:0", "QCOM:7", "QCOMX",
        "CPUfoo", "AMD", "DISK:/x", "cpu", "",
    ] {
        for allocated in [false, true] {
            let value = Descriptor::Single {
                buffer: buffer(7, 8, 2, 3),
                device,
                allocated,
            };
            let host = matches!(device.split(':').next(), Some("CPU" | "NPY" | "PYTHON"));
            assert_eq!(host_buffer(value).is_ok(), host, "{device} {allocated}");
            assert_eq!(
                descriptor(value).is_ok(),
                (host || device.split(':').next() == Some("QCOM"))
                    && !(device == "QCOM" && allocated),
                "{device} {allocated}"
            );
        }
    }
}

#[test]
fn offline_reads_use_base_offsets_and_union_of_prior_alias_writes() {
    let initialized = BTreeMap::from([(9, vec![10, 11, 12, 13, 14, 15, 16, 17])]);
    let mut table = OfflineBufferTable::new(&initialized);
    table.write(buffer(9, 8, 2, 2)).unwrap();
    table.write(buffer(9, 8, 3, 3)).unwrap();
    table.read(buffer(9, 8, 1, 6)).unwrap();
    assert_eq!(table.view(buffer(9, 8, 1, 6)).unwrap(), 2);
    assert_eq!(table.views().len(), 3);
    assert_eq!(table.weights().1, [0, 11, 0, 0, 0, 0, 16, 0]);
}

#[test]
fn missing_wrong_length_and_wrong_identity_initializers_are_checked_lazily() {
    for initialized in [
        BTreeMap::new(),
        BTreeMap::from([(7, vec![1; 3])]),
        BTreeMap::from([(7, vec![1; 5])]),
        BTreeMap::from([(8, vec![1; 4])]),
    ] {
        let mut table = OfflineBufferTable::new(&initialized);
        assert!(table.read(buffer(7, 4, 0, 4)).is_err());
        // A failed read still interns its view, just as the original table does.
        assert_eq!(table.view(buffer(7, 4, 0, 4)).unwrap(), 0);
        table.write(buffer(7, 4, 0, 2)).unwrap();
        assert!(table.read(buffer(7, 4, 0, 4)).is_err());
        table.write(buffer(7, 4, 2, 2)).unwrap();
        table.read(buffer(7, 4, 0, 4)).unwrap();
        assert!(table.weights().1.is_empty());
    }
}

#[test]
fn zero_length_offline_read_preserves_original_lazy_validation_order() {
    let initialized = BTreeMap::new();
    let mut table = OfflineBufferTable::new(&initialized);
    assert!(table.read(buffer(7, 0, 0, 0)).is_err());
    table.write(buffer(7, 0, 0, 0)).unwrap();
    table.read(buffer(7, 0, 0, 0)).unwrap();
    assert_eq!(table.weights().0[0].bytes, 0);
}

#[test]
fn weights_keep_encounter_order_and_raw_zero_bits() {
    let initialized = BTreeMap::from([
        (90, vec![0; 4]),
        (2, vec![0, 0, 0, 128]),
        (1, vec![7, 0, 0]),
    ]);
    let mut table = OfflineBufferTable::new(&initialized);
    for (id, bytes) in [(90, 4), (2, 4), (1, 3)] {
        table.read(buffer(id, bytes, 0, bytes)).unwrap();
    }
    let (allocations, packed) = table.weights();
    assert_eq!(
        allocations
            .iter()
            .map(|v| v.weight_offset)
            .collect::<Vec<_>>(),
        [None, Some(0), Some(4)]
    );
    assert_eq!(packed, [0, 0, 0, 128, 7, 0, 0]);
    assert_eq!(table.weights().1, packed);
}

#[test]
fn host_reads_recapture_unwritten_bytes_without_marking_them_written() {
    let mut table = BufferTable::default();
    table.read(buffer(7, 4, 1, 2), &[1, 2]).unwrap();
    table.read(buffer(7, 4, 1, 2), &[3, 4]).unwrap();
    table.write(buffer(7, 4, 1, 1)).unwrap();
    table.read(buffer(7, 4, 1, 2), &[9, 10]).unwrap();
    assert_eq!(table.weights().1, [0, 3, 10, 0]);
    table.write(buffer(8, 4, 0, 4)).unwrap();
    table.read(buffer(8, 4, 0, 4), &[]).unwrap();
}

fn bytes(value: &Value) -> Vec<u8> {
    serde_json::from_value(value.clone()).unwrap()
}

fn native_trace(case: &Value) -> Value {
    let bases = case["bases"].as_array().unwrap();
    let mut initialized = bases
        .iter()
        .filter_map(|base| {
            base.get("initialized")
                .map(|data| (base["id"].as_u64().unwrap(), bytes(data)))
        })
        .collect::<BTreeMap<_, _>>();
    for base in bases {
        if let Some(alias) = base.get("initializer_alias") {
            initialized.insert(alias["id"].as_u64().unwrap(), bytes(&alias["data"]));
        }
    }
    let host = case["kind"] == "host";
    let mut captured = bases
        .iter()
        .filter_map(|base| {
            base.get("data")
                .map(|data| (base["id"].as_u64().unwrap(), bytes(data)))
        })
        .collect::<BTreeMap<_, _>>();
    let mut table = BufferTable::default();
    let mut offline = OfflineBufferTable::new(&initialized);
    let mut trace = Vec::new();
    for op in case["ops"].as_array().unwrap() {
        let base = op["base"].as_u64().unwrap();
        let allocation = bases
            .iter()
            .find(|value| value["id"].as_u64() == Some(base))
            .unwrap();
        let value = buffer(
            base,
            allocation["bytes"].as_u64().unwrap() as usize,
            op["offset"].as_u64().unwrap() as usize,
            op["bytes"].as_u64().unwrap() as usize,
        );
        if let Some(data) = op.get("data") {
            captured.insert(base, bytes(data));
        }
        let result = match (host, op["op"].as_str().unwrap()) {
            (true, "view") => table.view(value).map(|index| json!(index)),
            (true, "write") => table.write(value).map(|_| Value::Null),
            (true, "read") => table
                .read(
                    value,
                    &captured[&base][value.offset..value.offset + value.bytes],
                )
                .map(|_| Value::Null),
            (false, "view") => offline.view(value).map(|index| json!(index)),
            (false, "write") => offline.write(value).map(|_| Value::Null),
            (false, "read") => offline.read(value).map(|_| Value::Null),
            _ => panic!("unknown recipe operation"),
        };
        let outcome = match result {
            Ok(value) => json!({"ok":value}),
            Err(_) => json!({"error":true}),
        };
        let (views, (allocations, weights)) = if host {
            (&table.views[..], table.weights())
        } else {
            (offline.views(), offline.weights())
        };
        trace.push(
            json!({"outcome":outcome,"views":views,"allocations":allocations,"weights":weights}),
        );
    }
    json!(trace)
}

fn op(name: &str, base: u64, offset: usize, bytes: usize) -> Value {
    json!({"op":name,"base":base,"offset":offset,"bytes":bytes})
}

// source100의 실제 원본 1회 관측이다. oracle.txt는 실행하지 않는 provenance 데이터다.
// 현재 원본22파일/Python/GHA 환경을 열거나 호출하지 않고 아래 고정 bytes만 검증한다.
const FROZEN_SOURCE: &str = "1000464f57239a1d15bfba3bdb6c01550f12d8a0";
const FROZEN_ROOT: &str = "/home/actions/actions-runner/_work/openpilot-rust/openpilot-rust/.diag296-source-40231559285-1.AaaMkI/source";
const FROZEN_JOB: &str = "/tmp/openpilot-ci.kSO6zn";
const FROZEN_PYTHON: &str = "/usr/bin/python3.12";
const FROZEN_PYTHON_SHA256: &str =
    "e50d468e8b0adfb05733f5b87b3cff34829c4a8c1aea50c865aa8bdfe4bb150f";
const RECORD_LIMIT: usize = 1024 * 1024;
const CORPUS_LIMIT: usize = 8 * 1024 * 1024;
const FROZEN_RECORDS: [(&str, usize, &str, &[u8]); 7] = [
    (
        "observation.json",
        39344,
        "e8e52475eba99d6e13287ecb957ec650c8e4d64b1e517caea1fd3b08fdddf257",
        include_bytes!("fixtures/compiler-buffer-original/observation.json"),
    ),
    (
        "oracle.txt",
        4694,
        "2b4747903018e15f3f99eabf62a6fb052f4899bfd0bee1ac44ce7bb5501cf371",
        include_bytes!("fixtures/compiler-buffer-original/oracle.txt"),
    ),
    (
        "python.sha256",
        86,
        "74a2bf284a5116d43270762ab1766fe74421da338fa0889fcd2c188202eae773",
        include_bytes!("fixtures/compiler-buffer-original/python.sha256"),
    ),
    (
        "python.version",
        14,
        "5b3e43dc38ca01e3f5a7854ba50d4330864b0c1e0f646650b2c78cf072acc366",
        include_bytes!("fixtures/compiler-buffer-original/python.version"),
    ),
    (
        "request.json",
        60636,
        "809ff0aba501e93a4bab0d86fd8066a77d72881580cfc72408e1c1ea19fd15fb",
        include_bytes!("fixtures/compiler-buffer-original/request.json"),
    ),
    (
        "stderr.bin",
        0,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        include_bytes!("fixtures/compiler-buffer-original/stderr.bin"),
    ),
    (
        "stdout.bin",
        270242,
        "fc019a80d96e11a95cc8d1573b94cbbb4ea06f30b56fdf71efa6f29d5e149593",
        include_bytes!("fixtures/compiler-buffer-original/stdout.bin"),
    ),
];

// source100에서 인증한 과거 source pin이며 변경된 test나 현재 원본을 다시 읽지 않는다.
const FROZEN_SOURCE_FILES: [(&str, usize, &str); 22] = [
    (
        "rust/Cargo.lock",
        159864,
        "9586516b4adda7abc3f739f2c3360a6cd2370e8f2d785a6b36a4adfcf54f6b9a",
    ),
    (
        "rust/crates/model-runtime/Cargo.toml",
        1605,
        "53d766c5fa51dab5e4be7c7b793e82d591a0bf7970784b68638c9b5159e56866",
    ),
    (
        "rust/crates/model-runtime/src/compiler_buffers.rs",
        3332,
        "1588afe7d4d6d69bac36df96653405802d569e730812dd001ffc874ebcdf42da",
    ),
    (
        "rust/crates/model-runtime/src/export.rs",
        24746,
        "7dc0993039db5247aa0e71d7c122c46b393e30a8e02724f8367a3b26ec6f6e8c",
    ),
    (
        "rust/crates/model-runtime/src/lib.rs",
        1118,
        "9c18a2434644ea72ebbd5d2d23c706084391775995e0567c6a0d1809f8f1d741",
    ),
    (
        "rust/crates/model-runtime/tests/compiler_buffers.rs",
        31176,
        "31c68a1b54d0162837329b374752a765b22a96710a385458f527a0c6fd9dd950",
    ),
    (
        "rust/tools/model_export/buffers.py",
        2577,
        "9429df49f408cda89f5b862fc0d51982668234cc18cb28ca0e83fd3d1a5bc7de",
    ),
    (
        "rust/tools/model_export/offline_buffers.py",
        1575,
        "9ab6c8c777b04d374750779fabfa75bb2b2af64fb6f8ce3ff8edd7a14138de4f",
    ),
    (
        "rust/tools/model_export/schema.py",
        2611,
        "720abcc396852a395689c4d37f9487822f951166a36b5efe8586dce5f656c6a9",
    ),
    (
        "tinygrad_repo/tinygrad/__init__.py",
        674,
        "fc24d5d0df1b1a14bbee0bd2641a61ca5f34ec7d83f1b0b755adf6eb83d5c0be",
    ),
    (
        "tinygrad_repo/tinygrad/device.py",
        20639,
        "7c28ae3854a3f009fc6eddfb52f03b3d382a23b581d94a3c4741a12bdc79f008",
    ),
    (
        "tinygrad_repo/tinygrad/dtype.py",
        20162,
        "de4bb7980fac9c09f851374fc6e8c57441071fca6d98dc92c6e984170bc58008",
    ),
    (
        "tinygrad_repo/tinygrad/helpers.py",
        32632,
        "24c347d2137c92e1dc35d78cd9df45b38d31cc4d34729a35215f8822b3f2f6fb",
    ),
    (
        "tinygrad_repo/tinygrad/renderer/__init__.py",
        4405,
        "22b219d73973e2591201662ae4a5c4ebd8688e51313a0ad4f55d50ae513828f7",
    ),
    (
        "tinygrad_repo/tinygrad/renderer/cstyle.py",
        37506,
        "13d806a4674ec87ffc94959b5825bb5915a7581b9c6652a3ddfe8a52120470be",
    ),
    (
        "tinygrad_repo/tinygrad/runtime/autogen/libc.py",
        124407,
        "89004d7b83537131ef45a55191eb1ce2708ef5e3f649abf656abdeeb193d42db",
    ),
    (
        "tinygrad_repo/tinygrad/runtime/ops_cpu.py",
        8395,
        "012f3d932abba9e14f82aedefe0ca52243eba1728f847c38cae54ae05b171a16",
    ),
    (
        "tinygrad_repo/tinygrad/runtime/support/elf.py",
        5263,
        "ed0340b507c8fb23f1fa1ae1b3ca1297006d70c289a016233d9f1d5291801aa8",
    ),
    (
        "tinygrad_repo/tinygrad/runtime/support/hcq.py",
        32420,
        "86ccce42fce7fe59f4cd2e345d9186b7241e44a00763ee51712c16a10b521bff",
    ),
    (
        "tinygrad_repo/tinygrad/runtime/support/memory.py",
        15268,
        "0457d300a3f149ba11b2da97240f7c7b6c0bb952e7ac4779dea2cfc0c44c81e2",
    ),
    (
        "tinygrad_repo/tinygrad/uop/__init__.py",
        4829,
        "8de567435d8c37f5472650e96409660063890d7a1a6dea32054396da6b6749ac",
    ),
    (
        "tinygrad_repo/tinygrad/uop/ops.py",
        94967,
        "d4b454cc4577a23be8d688aed874b9911992332e21bea7d7567974c9dc7c197a",
    ),
];

fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

fn frozen_bytes(name: &str) -> &'static [u8] {
    FROZEN_RECORDS
        .iter()
        .find(|record| record.0 == name)
        .unwrap()
        .3
}

fn assert_frozen_witness(witness: &Value, path: &str, bytes: usize, digest: &str) {
    assert_eq!(witness["path"], path);
    assert_eq!(witness["sha256"], digest);
    assert_eq!(witness["identity"]["bytes"], bytes);
    assert_eq!(witness["identity"]["links"], 1);
    assert_eq!(
        witness["identity"]["mode"].as_u64().unwrap() & 0o170000,
        0o100000
    );
}

fn frozen_reference(request: &[u8]) -> Value {
    let mut total = 0;
    for &(name, expected_bytes, digest, bytes) in &FROZEN_RECORDS {
        assert_eq!(bytes.len(), expected_bytes, "frozen bytes: {name}");
        assert_eq!(sha256(bytes), digest, "frozen SHA256: {name}");
        assert!(bytes.len() <= RECORD_LIMIT);
        total += bytes.len();
    }
    assert!(total <= CORPUS_LIMIT);
    assert_eq!(total, 375016);
    // 원문 SHA는 위에서 고정한다. 입력 object key 순서는 preserve_order feature에
    // 영향을 받지만 의미는 같으며, array 순서·값·숫자/boolean 종류는 그대로 대조한다.
    assert_eq!(
        serde_json::from_slice::<Value>(request).unwrap(),
        serde_json::from_slice::<Value>(frozen_bytes("request.json")).unwrap()
    );
    let observation: Value = serde_json::from_slice(frozen_bytes("observation.json")).unwrap();
    assert_eq!(observation["schema_version"], 1);
    assert_eq!(observation["kind"], "independent-python-buffer-observation");
    assert_eq!(observation["github_sha"], FROZEN_SOURCE);
    assert_eq!(observation["source_root"], FROZEN_ROOT);
    assert_eq!(observation["binding_unchanged"], true);
    assert_eq!(observation["collector_only"], true);
    assert_eq!(observation["retirement_credit"], 0);
    assert_eq!(observation["record_limit_bytes"], RECORD_LIMIT);
    assert_eq!(observation["total_limit_bytes"], CORPUS_LIMIT);
    assert_eq!(
        observation["request"],
        json!({
            "record":"request.json", "record_unchanged":true,
            "sha256":sha256(frozen_bytes("request.json"))
        })
    );
    assert_eq!(
        observation["original"],
        json!({
            "exit_code":0,"returncode":0,"signal":null,
            "file_size_limit_signal":false,"input_error":null,
            "stdout":{"record":"stdout.bin","bytes":270242,"sha256":sha256(frozen_bytes("stdout.bin"))},
            "stderr":{"record":"stderr.bin","bytes":0,"sha256":sha256(frozen_bytes("stderr.bin"))}
        })
    );
    assert!(frozen_bytes("stderr.bin").is_empty());
    assert_eq!(
        observation["argv"],
        json!([
            FROZEN_PYTHON,
            "-I",
            "-B",
            "-c",
            std::str::from_utf8(frozen_bytes("oracle.txt")).unwrap(),
            format!("{FROZEN_ROOT}/rust/crates/model-runtime/../../..")
        ])
    );
    assert_eq!(frozen_bytes("python.version"), b"Python 3.12.3\n");
    assert_eq!(
        frozen_bytes("python.sha256"),
        format!("{FROZEN_PYTHON_SHA256}  {FROZEN_PYTHON}\n").as_bytes()
    );

    let before = &observation["witnesses_before"];
    assert_eq!(before, &observation["witnesses_after"]);
    assert_eq!(
        before.as_object().unwrap().len(),
        FROZEN_SOURCE_FILES.len() + 5
    );
    for &(name, bytes, digest) in &FROZEN_SOURCE_FILES {
        assert_frozen_witness(
            &before[name],
            &format!("{FROZEN_ROOT}/{name}"),
            bytes,
            digest,
        );
    }
    assert_frozen_witness(
        &before["admitted-python"],
        FROZEN_PYTHON,
        8020928,
        FROZEN_PYTHON_SHA256,
    );
    for (name, bytes) in [
        ("SOURCE_COMMIT", format!("{FROZEN_SOURCE}\n").into_bytes()),
        (
            "buffer-oracle-python.sha256",
            frozen_bytes("python.sha256").to_vec(),
        ),
        (
            "buffer-oracle-python.version",
            frozen_bytes("python.version").to_vec(),
        ),
    ] {
        assert_frozen_witness(
            &before[name],
            &format!("{FROZEN_JOB}/proof/{name}"),
            bytes.len(),
            &sha256(&bytes),
        );
    }
    let owner = b"40231559285:1:native-units";
    assert_frozen_witness(
        &before["job-owner"],
        &format!("{FROZEN_JOB}/owner"),
        owner.len(),
        &sha256(owner),
    );

    let input: Value = serde_json::from_slice(frozen_bytes("request.json")).unwrap();
    assert_eq!(input.as_object().unwrap().len(), 2);
    assert_eq!(input["descriptors"].as_array().unwrap().len(), 32);
    assert_eq!(input["cases"].as_array().unwrap().len(), 147);
    let reference: Value = serde_json::from_slice(frozen_bytes("stdout.bin")).unwrap();
    assert_eq!(reference.as_object().unwrap().len(), 4);
    assert_eq!(reference["source_sha256"].as_object().unwrap().len(), 6);
    for (name, digest) in reference["source_sha256"].as_object().unwrap() {
        assert_eq!(
            *digest,
            before[name.as_str()]["sha256"],
            "original source binding: {name}"
        );
    }
    assert_eq!(reference["opened_devices"], json!(["CPU"]));
    assert_eq!(reference["descriptors"].as_array().unwrap().len(), 32);
    let traces = reference["traces"].as_array().unwrap();
    assert_eq!(traces.len(), 147);
    assert_eq!(
        traces
            .iter()
            .map(|trace| trace.as_array().unwrap().len())
            .sum::<usize>(),
        959
    );
    for (case, trace) in input["cases"].as_array().unwrap().iter().zip(traces) {
        assert_eq!(
            case["ops"].as_array().unwrap().len(),
            trace.as_array().unwrap().len()
        );
    }
    reference
}

#[test]
fn frozen_original_buffer_tables_match_native() {
    let mut descriptors = Vec::new();
    for device in [
        "CPU", "CPU:0", "CPU:LLVM", "NPY", "NPY:2", "PYTHON", "QCOM", "QCOM:0", "QCOM:7", "QCOMX",
        "CPUfoo", "AMD", "DISK:/x", "cpu", "",
    ] {
        for allocated in [false, true] {
            descriptors.push(json!({"kind":"single","device":device,"allocated":allocated}));
        }
    }
    descriptors.extend([json!({"kind":"multi"}), json!({"kind":"other"})]);
    let mut cases = Vec::new();
    for kind in ["host", "offline"] {
        // All 64 write masks over six interior bytes. Reads cross both edges;
        // distinct aliased objects repeat identical ranges in original Python.
        for mask in 0..64usize {
            let mut ops = (0..6)
                .filter(|index| mask & (1usize << *index) != 0)
                .map(|index| op("write", 9, index + 1, 1))
                .collect::<Vec<_>>();
            ops.extend([
                op("read", 9, 0, 8),
                op("view", 9, 2, 3),
                op("view", 9, 2, 3),
                op("read", 9, 1, 6),
            ]);
            cases.push(json!({"kind":kind,"bases":[{"id":9,"bytes":8,"data":[1,2,3,4,5,6,7,8],"initialized":[1,2,3,4,5,6,7,8]}],"ops":ops}));
        }
        for ops in [
            vec![
                op("write", 9, 1, 4),
                op("write", 9, 3, 4),
                op("read", 9, 0, 8),
            ],
            vec![
                op("read", 9, 2, 3),
                op("write", 9, 3, 2),
                op("read", 9, 0, 8),
            ],
            vec![op("write", 9, 0, 8), op("read", 9, 0, 8)],
        ] {
            cases.push(json!({"kind":kind,"bases":[{"id":9,"bytes":8,"data":[0,1,0,2,0,3,0,4],"initialized":[0,1,0,2,0,3,0,4]}],"ops":ops}));
        }
        cases.push(json!({"kind":kind,"bases":[
            {"id":90,"bytes":4,"data":[0,0,0,0],"initialized":[0,0,0,0]},
            {"id":2,"bytes":4,"data":[0,0,0,128],"initialized":[0,0,0,128]},
            {"id":1,"bytes":3,"data":[0,255,0],"initialized":[0,255,0]},
            {"id":3,"bytes":4,"data":[0,0,0,128],"initialized":[0,0,0,128]}],
            "ops":[op("read",90,0,4),op("read",2,0,4),op("read",1,0,3),op("read",3,0,4)]}));
    }
    // Missing/short/long initialization: error before full write coverage, then
    // success without consulting missing/malformed bytes after complete writes.
    for data in [Value::Null, json!([1, 2, 3]), json!([1, 2, 3, 4, 5])] {
        let mut base = json!({"id":7,"bytes":4});
        if !data.is_null() {
            base["initialized"] = data;
        }
        cases.push(json!({"kind":"offline","bases":[base],"ops":[op("read",7,0,4),op("write",7,0,2),op("read",7,0,4),op("write",7,2,2),op("read",7,0,4)]}));
    }
    for device in ["CPU", "NPY", "PYTHON", "QCOM:7"] {
        cases.push(json!({"kind":"offline","bases":[{"id":7,"bytes":4,"device":device}],"ops":[op("read",7,0,4),op("write",7,0,4),op("read",7,0,4)]}));
    }
    cases.push(json!({"kind":"offline","bases":[{"id":7,"bytes":4},{"id":8,"bytes":4,"initialized":[1,2,3,4]}],"ops":[op("read",7,0,4),op("read",8,0,4)]}));
    cases.push(json!({"kind":"offline","bases":[{"id":7,"bytes":4,"initializer_alias":{"id":700,"offset":1,"bytes":2,"data":[1,2,3,4]}}],"ops":[op("read",7,0,4),op("write",7,0,4),op("read",7,0,4)]}));
    cases.push(json!({"kind":"offline","bases":[{"id":7,"bytes":0}],"ops":[op("read",7,0,0),op("write",7,0,0),op("read",7,0,0)]}));
    cases.push(json!({"kind":"host","bases":[{"id":7,"bytes":4,"data":[1,2,3,4]}],"ops":[op("read",7,1,2),{"op":"read","base":7,"offset":1,"bytes":2,"data":[9,10,11,12]},op("write",7,1,1),{"op":"read","base":7,"offset":1,"bytes":2,"data":[20,21,22,23]}]}));
    assert_eq!(descriptors.len(), 32);
    assert_eq!(cases.len(), 147);
    let request = json!({"descriptors":descriptors,"cases":cases});
    let request_bytes = serde_json::to_vec(&request).unwrap();
    let reference = frozen_reference(&request_bytes);
    assert_eq!(reference["opened_devices"], json!(["CPU"]));
    assert_eq!(
        reference["descriptors"].as_array().unwrap().len(),
        descriptors.len()
    );
    for (case, expected) in descriptors
        .iter()
        .zip(reference["descriptors"].as_array().unwrap())
    {
        let value = if case["kind"] == "single" {
            Descriptor::Single {
                buffer: buffer(7, 8, 0, 8),
                device: case["device"].as_str().unwrap(),
                allocated: case["allocated"].as_bool().unwrap(),
            }
        } else {
            Descriptor::NonSingle
        };
        assert_eq!(
            json!({"host_buffer":host_buffer(value).is_ok(),"descriptor":descriptor(value).is_ok()}),
            *expected,
            "{case}"
        );
    }
    assert_eq!(reference["traces"].as_array().unwrap().len(), cases.len());
    for (index, (case, expected)) in cases
        .iter()
        .zip(reference["traces"].as_array().unwrap())
        .enumerate()
    {
        assert_eq!(native_trace(case), *expected, "trace {index}: {case}");
    }
}
