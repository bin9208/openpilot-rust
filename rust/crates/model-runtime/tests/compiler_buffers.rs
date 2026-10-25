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

// All expected traces below are computed by calling the original Python table
// methods on genuine tinygrad Buffer objects. Only import side effects are
// avoided by extracting unchanged definitions; no candidate outputs enter the
// original process. CPU allocation/copyout is real; no kernel is compiled/run.
const ORIGINAL_BUFFERS: &str = r#"
import ast, dataclasses, hashlib, json, os, pathlib, sys, types
from collections.abc import Mapping
root = pathlib.Path(sys.argv[1])
os.environ.update(DEV='CPU', CACHELEVEL='0', DEBUG='0', PROFILE='0', PYTHONDONTWRITEBYTECODE='1', TYPED='0')
sys.path.insert(0, str(root))
sys.path.insert(0, str(root/'tinygrad_repo'))
from tinygrad.device import Buffer, MultiBuffer, Device, Compiler
from tinygrad.dtype import dtypes
from tinygrad.uop.ops import UOp
def no_compilation(*args, **kwargs):
    raise AssertionError('buffer oracle must not compile kernels')
Compiler.compile = Compiler.compile_cached = no_compilation
open_device = Device._Device__get_canonicalized_item
def only_cpu(name):
    assert name.split(':')[0] == 'CPU', f'buffer oracle must not open {name}'
    return open_device(name)
Device._Device__get_canonicalized_item = only_cpu
scope = {'__name__': '__main__', 'dataclass': dataclasses.dataclass, 'Mapping': Mapping,
         'Buffer': Buffer, 'MultiBuffer': MultiBuffer, 'UOp': UOp}
hashes = {}
def definitions(relative, names):
    path = root/relative
    content = path.read_bytes()
    hashes[relative] = hashlib.sha256(content).hexdigest()
    tree = ast.parse(content, filename=str(path))
    for name in names:
        node = next(node for node in tree.body if getattr(node, 'name', None) == name)
        exec(compile(ast.Module(body=[node], type_ignores=[]), str(path), 'exec'), scope)
definitions('rust/tools/model_export/schema.py', ['ExportError', 'Allocation', 'View'])
definitions('rust/tools/model_export/buffers.py', ['host_buffer', 'BufferTable'])
definitions('rust/tools/model_export/offline_buffers.py', ['descriptor', 'OfflineBufferTable'])
hashes['tinygrad_repo/tinygrad/device.py'] = hashlib.sha256((root/'tinygrad_repo/tinygrad/device.py').read_bytes()).hexdigest()
for relative in ('tinygrad_repo/tinygrad/runtime/ops_cpu.py', 'tinygrad_repo/tinygrad/runtime/support/hcq.py'):
    hashes[relative] = hashlib.sha256((root/relative).read_bytes()).hexdigest()
request = json.load(sys.stdin)
descriptor_results = []
for case in request['descriptors']:
    if case['kind'] == 'multi':
        value = MultiBuffer(('CPU', 'PYTHON'), 8, dtypes.uint8)
    elif case['kind'] == 'other':
        value = object()
    else:
        value = Buffer(case['device'], 8, dtypes.uint8)
        if case['allocated']:
            # Synthetic allocation-state control only: no GPU/host provider is
            # opened for descriptor tests. Clear it before object finalization.
            value._bufs[value.device] = object()
    row = {}
    for name, argument in [('host_buffer', value), ('descriptor', types.SimpleNamespace(buffer=value))]:
        try:
            row[name] = scope[name](argument) is value
        except scope['ExportError']:
            row[name] = False
    descriptor_results.append(row)
    if isinstance(value, Buffer):
        value._bufs.clear()
traces = []
for case in request['cases']:
    host = case['kind'] == 'host'
    bases = {base['id']: Buffer('CPU' if host else base.get('device', 'QCOM'), base['bytes'], dtypes.uint8)
             for base in case['bases']}
    initial = {bases[base['id']]: bytes(base['initialized']) for base in case['bases'] if 'initialized' in base}
    for base in case['bases']:
        if 'initializer_alias' in base:
            alias = base['initializer_alias']
            owner = bases[base['id']]
            key = Buffer(owner.device, alias['bytes'], dtypes.uint8, base=owner, offset=alias['offset'])
            initial[key] = bytes(alias['data'])
    table = scope['BufferTable']() if host else scope['OfflineBufferTable'](initial)
    if host:
        for base in case['bases']:
            bases[base['id']].ensure_allocated().copyin(memoryview(bytes(base['data'])))
    rows = []
    for op in case['ops']:
        base = bases[op['base']]
        view = Buffer(base.device, op['bytes'], dtypes.uint8, base=base, offset=op['offset'])
        if host and 'data' in op:
            base.copyin(memoryview(bytes(op['data'])))
        try:
            returned = getattr(table, op['op'])(view)
            outcome = {'ok': returned}
        except scope['ExportError']:
            outcome = {'error': True}
        allocations, packed = table.weights()
        rows.append({'outcome': outcome, 'views': [dataclasses.asdict(v) for v in table.views],
                     'allocations': [dataclasses.asdict(a) for a in allocations], 'weights': list(packed)})
    traces.append(rows)
assert Device._opened_devices == {'CPU'}
print(json.dumps({'source_sha256': hashes, 'opened_devices': sorted(Device._opened_devices),
                  'descriptors': descriptor_results, 'traces': traces}))
"#;

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

#[test]
#[ignore = "GHA-only: genuine Python buffer table methods, CPU byte copies only, no kernel/model execution"]
fn original_python_buffer_tables_match_native() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
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
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let mut child = Command::new("python3")
        .args(["-I", "-B", "-c", ORIGINAL_BUFFERS])
        .arg(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Actions must provide python3");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&request).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let reference: Value = serde_json::from_slice(&output.stdout).unwrap();
    eprintln!("original buffer sources: {}", reference["source_sha256"]);
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
