#![cfg(feature = "native-skip-miri")]

use openpilot_model_runtime::qcom::{QcomBundle, QcomGraph};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf};

fn fixture() -> Vec<u8> {
    fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/qcom/buffer_add.bin"))
        .unwrap()
}

fn manifest() -> Value {
    let binary = fixture();
    json!({"version":1,"backend":"qcom-cl","arch":"a630","weights_sha256":format!("{:x}",Sha256::digest([])),
        "allocations":[{"bytes":64,"weight_offset":null}],
        "views":[{"allocation":0,"offset":0,"bytes":32},{"allocation":0,"offset":32,"bytes":32}],
        "inputs":[{"name":"input","view":0}],"outputs":[{"name":"output","view":1}],
        "kernels":[{"name":"buffer_add","binary_sha256":format!("{:x}",Sha256::digest(&binary)),"binary_bytes":binary.len(),
            "arguments":[[{"kind":"buffer"}],[{"kind":"buffer"}]]}],
        "calls":[{"op":"kernel","kernel":0,"views":[1,0],"scalars":[],"global":[8,1,1],"local":[1,1,1]},
            {"op":"copy","source":1,"destination":0}]})
}

fn bundle(manifest: &Value) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("weights.bin"), []).unwrap();
    fs::write(directory.path().join("kernel-0.bin"), fixture()).unwrap();
    fs::write(
        directory.path().join("graph.json"),
        serde_json::to_vec(manifest).unwrap(),
    )
    .unwrap();
    directory
}

#[test]
fn validates_executable_bundle_without_opening_a_gpu() {
    let directory = bundle(&manifest());
    let loaded = QcomBundle::load(directory.path()).unwrap();
    assert_eq!(loaded.allocation_bytes(), 64);
    assert_eq!(loaded.call_count(), 2);
    assert_eq!(loaded.kernel_count(), 1);
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_qcom-model-run"))
        .arg("--check-bundle")
        .arg(directory.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["gpu_executed"], false);
    assert_eq!(report["calls"], 2);
}

#[test]
fn rejects_corrupt_qcom_assets_and_invalid_kernel_signatures() {
    let directory = bundle(&manifest());
    fs::write(directory.path().join("kernel-0.bin"), [0; 64]).unwrap();
    assert!(QcomBundle::load(directory.path()).is_err());
    let directory = bundle(&manifest());
    fs::write(directory.path().join("weights.bin"), [1]).unwrap();
    assert!(QcomBundle::load(directory.path()).is_err());
    let mut wrong = manifest();
    wrong["calls"][0]["scalars"] = json!([7]);
    let directory = bundle(&wrong);
    assert!(QcomBundle::load(directory.path()).is_err());
}

#[test]
fn rejects_invalid_qcom_ranges_and_dispatch_contracts() {
    for (path, value) in [
        ("/version", json!(2)),
        ("/backend", json!("cpu-clang")),
        ("/arch", json!("a730")),
        ("/allocations/0/bytes", json!(u64::MAX)),
        ("/allocations/0/weight_offset", json!(u64::MAX)),
        ("/views/1/offset", json!(33)),
        ("/views/1/allocation", json!(1)),
        ("/inputs/0/view", json!(2)),
        ("/calls/0/kernel", json!(1)),
        ("/calls/0/views/0", json!(2)),
        ("/calls/0/local/0", json!(0)),
        ("/calls/0/local", json!([1024, 1024, 1024])),
        ("/calls/0/global/0", json!(0)),
        ("/calls/1/source", json!(2)),
        ("/kernels/0/binary_bytes", json!(0)),
        (
            "/kernels/0/arguments/0",
            json!([{"kind":"image","width":64,"height":64,"pitch":1024,"element_bytes":4}]),
        ),
    ] {
        let mut graph = manifest();
        *graph.pointer_mut(path).unwrap() = value;
        assert!(
            QcomGraph::parse(&serde_json::to_vec(&graph).unwrap()).is_err(),
            "{path}"
        );
    }
}
