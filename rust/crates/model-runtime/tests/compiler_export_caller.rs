#![cfg(feature = "native-skip-miri")]
use openpilot_model_runtime::compiler_schedule::export::{prepare, Request};
use serde_json::{json, Value};

fn storage(base: u64, offset: usize, bytes: usize) -> Value {
    json!({"base":base,"allocation_bytes":8,"offset":offset,"bytes":bytes,"device":"QCOM","allocated":false})
}
fn request() -> Value {
    json!({"version":1,"initialized":[{"key":10,"data":"0102030405060708"}],
        "events":[{"op":"view","buffer":storage(20,0,8)},
                  {"op":"write","buffer":storage(10,0,4)},
                  {"op":"read","buffer":storage(10,0,8)},
                  {"op":"write","buffer":storage(20,0,8)},
                  {"op":"read","buffer":storage(20,0,8)}],
        "programs":[{"source":"get_local_id(0)","binary":""}],
        "launches":[{"program":0,"global":[{"type":"integer","value":"64"},{"type":"integer","value":"1"},{"type":"integer","value":"1"}],"local":null}],
        "emitted_by_source":[0,1],"stage_ends":[{"name":"prepare","end":0},{"name":"model","end":1}]})
}
fn plan(value: Value) -> Result<Value, String> {
    let request: Request = serde_json::from_value(value).map_err(|e| e.to_string())?;
    serde_json::to_value(prepare(request)?).map_err(|e| e.to_string())
}
#[test]
fn live_wire_composes_alias_weights_launch_and_stage_cores() {
    let result = plan(request()).unwrap();
    assert_eq!(result["weights"], "0000000005060708");
    assert_eq!(
        result["allocations"],
        json!([{"bytes":8,"weight_offset":null},{"bytes":8,"weight_offset":0}])
    );
    assert_eq!(result["event_views"], json!([0, 1, 2, 0, 0]));
    assert_eq!(
        result["entrypoints"],
        json!([{"name":"prepare","start":0,"end":0},{"name":"model","start":0,"end":1}])
    );
    assert_eq!(
        result["launches"][0]["local"],
        json!(vec![json!({"type":"integer","value":"1"}); 3])
    );
}
#[test]
fn live_wire_rejects_lazy_missing_bytes_bad_alias_and_allocated_qcom() {
    for mode in 0..5 {
        let mut value = request();
        match mode {
            0 => value["initialized"][0]["key"] = json!(99),
            1 => value["initialized"][0]["data"] = json!("00"),
            2 => value["events"][2]["buffer"]["allocation_bytes"] = json!(9),
            3 => value["events"][0]["buffer"]["allocated"] = json!(true),
            _ => value["stage_ends"][1]["end"] = json!(2),
        }
        assert!(plan(value).is_err(), "mode {mode}");
    }
    let mut value = request();
    value["events"][1]["buffer"] = storage(10, 0, 8);
    value["initialized"] = json!([]);
    assert_eq!(plan(value).unwrap()["weights"], "");
}

#[test]
fn existing_cli_exports_real_plan_and_rejects_invalid_request_without_output() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    for valid in [true, false] {
        let mut value = request();
        if !valid {
            value["version"] = json!(99);
        }
        let mut child = Command::new(env!("CARGO_BIN_EXE_offline-qcom-compile"))
            .args(["--library", "/unused-export-plan-provider", "--export-plan"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(&serde_json::to_vec(&value).unwrap())
            .unwrap();
        let result = child.wait_with_output().unwrap();
        if valid {
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert_eq!(
                serde_json::from_slice::<Value>(&result.stdout).unwrap(),
                plan(value).unwrap()
            );
        } else {
            assert!(!result.status.success());
            assert!(result.stdout.is_empty());
        }
    }
}

#[test]
fn existing_cli_preserves_explicit_launch_float_bits_and_dimension_types() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    const GLOBAL: f64 = 102.49635675101081;
    const LOCAL: f64 = 51.248178375505404;
    // Keep the decimal on the wire: an in-memory prepare() comparison alone
    // would miss serde_json's default parser changing the admitted f64 bits.
    // Explicit locals bypass regrouping in the current Rust/Python launch core.
    let request = br#"{"version":1,"initialized":[],"events":[],
        "programs":[{"source":"get_local_id(0)","binary":""}],
        "launches":[{"program":0,
            "global":[{"type":"float","value":102.49635675101081},{"type":"integer","value":"2"},{"type":"boolean","value":true}],
            "local":[{"type":"float","value":51.248178375505404},{"type":"float","value":1.0},{"type":"integer","value":"1"}]}],
        "emitted_by_source":[0,1],"stage_ends":[{"name":"model","end":1}]}"#;
    let mut child = Command::new(env!("CARGO_BIN_EXE_offline-qcom-compile"))
        .args(["--library", "/unused-export-plan-provider", "--export-plan"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(request).unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        std::str::from_utf8(&result.stdout)
            .unwrap()
            .contains("\"value\":51.248178375505404"),
        "CLI changed the regression decimal before response decoding"
    );
    let decoded: Value = serde_json::from_slice(&result.stdout).unwrap();
    let launch = &decoded["launches"][0];
    assert_eq!(
        launch,
        &json!({"global":[{"type":"float","value":GLOBAL},{"type":"integer","value":"2"},{"type":"boolean","value":true}],
            "local":[{"type":"float","value":LOCAL},{"type":"float","value":1.0},{"type":"integer","value":"1"}]})
    );
    for (dimension, expected) in [
        (&launch["global"][0], GLOBAL),
        (&launch["local"][0], LOCAL),
        (&launch["local"][1], 1.0_f64),
    ] {
        assert_eq!(dimension["type"], "float");
        assert!(dimension["value"].is_f64());
        assert_eq!(
            dimension["value"].as_f64().unwrap().to_bits(),
            expected.to_bits()
        );
    }
}

// The actual maintained Python emitter invokes the actual existing CLI. No
// model/compiler/provider/GPU is executed: a genuine COPY UOp needs no kernel.
const CALLER: &str = r#"
import json, pathlib, sys
root, binary, out = pathlib.Path(sys.argv[1]), sys.argv[2], pathlib.Path(sys.argv[3])
sys.path.insert(0, str(root))
sys.path.insert(0, str(root/'tinygrad_repo'))
sys.path.insert(0, str(root/'rust/tools'))
from tinygrad import Tensor, dtypes
from tinygrad.device import Device
from tinygrad.uop.ops import Ops, UOp
from model_export import Bindings
from model_export.offline_model import InitializedBuffer, OfflineModel
from model_export.offline_compiler import CompilerCommand, OfflineQcomCompiler
from model_export.offline_schedule import Emission, emit_schedule
opened = Device._Device__get_canonicalized_item
def no_gpu(name):
    assert not name.startswith('QCOM'), 'unexpected GPU allocation'
    return opened(name)
Device._Device__get_canonicalized_item = no_gpu
source = Tensor.empty(8, dtype=dtypes.uint8, device='QCOM')
target = Tensor.empty(8, dtype=dtypes.uint8, device='QCOM')
model = OfflineModel(Bindings({}, {'out':target}), (InitializedBuffer(source.uop.base, b'\x00\x01\x02\x03\x04\x05\x06\x07'),))
linear = UOp(Ops.LINEAR, src=(UOp(Ops.COPY).call(target.uop.buf_uop, source.uop.buf_uop),))
compiler = OfflineQcomCompiler(CompilerCommand((binary, '--library', str(out/'unused-provider')), 30))
result = emit_schedule(Emission(model, linear, (('empty',0),('copy',1))), compiler, out/'bundle')
graph = json.loads((out/'bundle/graph.json').read_bytes())
assert result.weights == (out/'bundle/weights.bin').read_bytes() == bytes(range(8))
assert graph['calls'] == [{'op':'copy', 'source':1, 'destination':0}]
assert graph['entrypoints'] == [{'name':'empty','start':0,'end':0},{'name':'copy','start':0,'end':1}]
assert len(result.programs) == 0 and graph['kernels'] == []
# Invalid native invocation must not fall back to the still-present Python table.
bad = OfflineQcomCompiler(CompilerCommand((binary, '--library', str(out/'unused-provider'), '--bad'), 30))
try:
    emit_schedule(Emission(model,linear),bad,out/'must-not-exist')
except RuntimeError as error:
    assert 'native export-plan failed' in str(error), str(error)
else:
    raise AssertionError('native failure accepted')
assert not (out/'must-not-exist').exists()
print('MAINTAINED_OFFLINE_EXPORT_NATIVE_COPY_PASS')
"#;

#[test]
#[ignore = "GHA-only actual Python maintained emitter and native CLI boundary; no inference"]
fn maintained_python_export_calls_native_plan_without_fallback() {
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let out = tempfile::tempdir().unwrap();
    let result = std::process::Command::new("python3")
        .args(["-I", "-B", "-c", CALLER])
        .arg(root)
        .arg(env!("CARGO_BIN_EXE_offline-qcom-compile"))
        .arg(out.path())
        .env("DEV", "CPU")
        .env("CACHELEVEL", "0")
        .env("DEBUG", "0")
        .env("PROFILE", "0")
        .env("TYPED", "0")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        String::from_utf8(result.stdout).unwrap().trim(),
        "MAINTAINED_OFFLINE_EXPORT_NATIVE_COPY_PASS"
    );
}
