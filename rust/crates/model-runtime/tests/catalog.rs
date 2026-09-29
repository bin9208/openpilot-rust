#![cfg(feature = "native-skip-miri")]

use openpilot_model_runtime::catalog::{Catalog, Kind};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

fn descriptor() -> Value {
    json!({"version":1,"kind":"driver","camera":[1344,760],
        "nv12":{"stride":1408,"y_height":768,"uv_height":384,"bytes":2428928},
        "sources":{"model":"11".repeat(32)},
        "metadata":{"model_checkpoint":"fixture","input_shapes":{"calib":[1,3]},
            "output_shapes":{"outputs":[1,1]},"output_slices":{"result":[0,1]}},
        "inputs":[{"name":"frame","shape":[2428928],"dtype":"uint8"}],
        "outputs":[{"name":"model","shape":[1,1],"dtype":"float32"}]})
}

fn fixture(descriptor: &Value, directory_name: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let graph = serde_json::to_vec(&json!({
        "version":2,"backend":"cpu-llvm","arch":std::env::consts::ARCH,
        "weights_sha256":"00".repeat(32),"library_sha256":"11".repeat(32),
        "allocations":[{"bytes":2428932,"weight_offset":null}],
        "views":[{"allocation":0,"offset":0,"bytes":2428928},{"allocation":0,"offset":2428928,"bytes":4}],
        "inputs":[{"name":"frame","view":0}],"outputs":[{"name":"model","view":1}],
        "kernels":[],"calls":[],
        "entrypoints":[{"name":"prepare","start":0,"end":0},{"name":"model","start":0,"end":0}]
    })).unwrap();
    let pipeline = serde_json::to_vec(descriptor).unwrap();
    let index = serde_json::to_vec(&json!({"version":1,"bundles":[{
        "kind":"driver","camera":[1344,760],"directory":directory_name,
        "graph_sha256":format!("{:x}",Sha256::digest(&graph)),
        "pipeline_sha256":format!("{:x}",Sha256::digest(&pipeline))
    }]}))
    .unwrap();
    let generation = format!("{:x}", Sha256::digest(&index));
    let path = root.path().join(&generation).join("driver-1344x760");
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("graph.json"), graph).unwrap();
    fs::write(path.join("pipeline.json"), pipeline).unwrap();
    fs::write(root.path().join(&generation).join("index.json"), index).unwrap();
    fs::write(
        root.path().join("current.json"),
        serde_json::to_vec(&json!({"version":1,"generation":generation})).unwrap(),
    )
    .unwrap();
    root
}

#[test]
fn selects_verified_metadata_without_loading_executable_kernels() {
    let root = fixture(&descriptor(), "driver-1344x760");
    let catalog = Catalog::load(root.path()).unwrap();
    let bundle = catalog.select(Kind::Driver, [1344, 760]).unwrap();
    assert_eq!(bundle.descriptor.metadata.output_slices["result"], [0, 1]);
    assert!(bundle.directory.ends_with("driver-1344x760"));
    assert!(catalog.select(Kind::Driving, [1344, 760]).is_err());
}

#[test]
fn rejects_metadata_that_disagrees_with_the_graph_or_camera() {
    for (pointer, replacement) in [
        ("/version", json!(2)),
        ("/kind", json!("driving")),
        ("/camera", json!([1928, 1208])),
        ("/nv12/bytes", json!(1)),
        ("/nv12/stride", json!(1409)),
        ("/inputs/0/shape", json!([2428927])),
        ("/inputs/0/name", json!("wrong")),
        ("/outputs/0/dtype", json!("float64")),
        ("/outputs/0/shape", json!([u64::MAX, 2])),
        ("/sources/model", json!("bad")),
        ("/metadata/output_slices/result", json!([0, 2])),
        ("/metadata/output_shapes/outputs", json!([1, 2])),
        ("/metadata/input_shapes/calib", json!([0, 3])),
    ] {
        let mut value = descriptor();
        *value.pointer_mut(pointer).unwrap() = replacement;
        let root = fixture(&value, "driver-1344x760");
        assert!(Catalog::load(root.path()).is_err(), "accepted {pointer}");
    }
}

#[test]
fn rejects_traversal_and_tampered_generation() {
    let root = fixture(&descriptor(), "../driver-1344x760");
    assert!(Catalog::load(root.path()).is_err());
    let root = fixture(&descriptor(), "driver-1344x760");
    let catalog = Catalog::load(root.path()).unwrap();
    let bundle = catalog.select(Kind::Driver, [1344, 760]).unwrap();
    fs::write(bundle.directory.join("pipeline.json"), b"{}").unwrap();
    assert!(Catalog::load(root.path()).is_err());
    fs::write(
        root.path().join("current.json"),
        br#"{"version":1,"generation":"../outside"}"#,
    )
    .unwrap();
    assert!(Catalog::load(root.path()).is_err());
}
