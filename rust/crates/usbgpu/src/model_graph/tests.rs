use super::{manifest::Manifest, Bundle};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

fn descriptor() -> Value {
    json!({
        "version":2,"backend":"amd-elf","arch":"gfx1200",
        "weights_sha256":"a".repeat(64),
        "allocations":[{"bytes":16,"weight_offset":null}],
        "views":[{"allocation":0,"offset":0,"bytes":16}],
        "inputs":[{"name":"frame","view":0}],"outputs":[{"name":"model","view":0}],
        "kernels":[{"name":"policy","binary_sha256":"b".repeat(64),"binary_bytes":16}],
        "calls":[{"op":"kernel","kernel":0,"views":[0],"scalars":[],"global":[1,1,1],"local":[1,1,1]}],
        "entrypoints":[{"name":"prepare","start":0,"end":0},{"name":"policy","start":0,"end":1}]
    })
}

#[test]
fn staged_graph_accepts_complete_partition() {
    // Given a graph with separately addressable prepare and policy stages.
    let bytes = serde_json::to_vec(&descriptor()).unwrap();
    // When the runtime parses its descriptor.
    let parsed = Manifest::parse(&bytes).unwrap();
    // Then both exact stage ranges are retained.
    assert_eq!(parsed.entry("prepare").unwrap(), 0..0);
    assert_eq!(parsed.entry("policy").unwrap(), 0..1);
}

#[test]
fn graph_rejects_invalid_asset_identity() {
    // Given a purported asset identity that is not a SHA-256 digest.
    let mut graph = descriptor();
    graph["kernels"][0]["binary_sha256"] = json!("not-a-digest");
    // When the runtime parses the descriptor.
    let result = Manifest::parse(&serde_json::to_vec(&graph).unwrap());
    // Then admission fails before allocation or execution.
    assert!(result.is_err());
}

#[test]
fn graph_rejects_entrypoint_overlap() {
    // Given two stages that claim the same call.
    let mut graph = descriptor();
    graph["entrypoints"][0]["end"] = json!(1);
    // When the runtime parses the descriptor.
    let result = Manifest::parse(&serde_json::to_vec(&graph).unwrap());
    // Then an ambiguous stage partition is rejected.
    assert!(result.is_err());
}

#[test]
fn graph_rejects_view_outside_allocation() {
    // Given an input view that extends past its backing allocation.
    let mut graph = descriptor();
    graph["views"][0]["offset"] = json!(1);
    // When the runtime parses the descriptor.
    let result = Manifest::parse(&serde_json::to_vec(&graph).unwrap());
    // Then the out-of-range view is rejected.
    assert!(result.is_err());
}

#[test]
fn graph_rejects_mismatched_copy_length() {
    // Given a copy between two differently sized views.
    let mut graph = descriptor();
    graph["views"]
        .as_array_mut()
        .unwrap()
        .push(json!({"allocation":0,"offset":0,"bytes":8}));
    graph["calls"][0] = json!({"op":"copy","source":0,"destination":1});
    // When the runtime parses the descriptor.
    let result = Manifest::parse(&serde_json::to_vec(&graph).unwrap());
    // Then the copy is rejected before buffers can be accessed.
    assert!(result.is_err());
}

fn package(directory: &std::path::Path) -> Value {
    let mut graph = descriptor();
    let kernel = b"owned-test-code!";
    graph["weights_sha256"] = json!(format!("{:x}", Sha256::digest([])));
    graph["kernels"][0]["binary_sha256"] = json!(format!("{:x}", Sha256::digest(kernel)));
    std::fs::write(directory.join("weights.bin"), []).unwrap();
    std::fs::write(directory.join("kernel-0.bin"), kernel).unwrap();
    std::fs::write(
        directory.join("graph.json"),
        serde_json::to_vec(&graph).unwrap(),
    )
    .unwrap();
    graph
}

#[test]
fn bundle_rejects_mutated_kernel_before_gpu_open() {
    // Given a package whose kernel changed after its descriptor was emitted.
    let directory = tempfile::tempdir().unwrap();
    package(directory.path());
    std::fs::write(directory.path().join("kernel-0.bin"), b"mutated-testcode").unwrap();
    // When the standalone admission boundary opens the package.
    let result = Bundle::open(directory.path());
    // Then it rejects the package without requiring a GPU.
    assert!(result.is_err());
}

#[test]
fn bundle_rejects_missing_initial_weight_range_before_gpu_open() {
    // Given a correctly hashed empty weight file and a nonempty initial range.
    let directory = tempfile::tempdir().unwrap();
    let mut graph = package(directory.path());
    graph["allocations"][0]["weight_offset"] = json!(0);
    std::fs::write(
        directory.path().join("graph.json"),
        serde_json::to_vec(&graph).unwrap(),
    )
    .unwrap();
    // When the standalone admission boundary opens the package.
    let result = Bundle::open(directory.path());
    // Then it rejects the unavailable bytes before any allocation.
    assert!(result.is_err());
}

#[test]
fn bundle_exposes_validated_binding_contract() {
    // Given a complete package with matching kernel and weight identities.
    let directory = tempfile::tempdir().unwrap();
    package(directory.path());
    // When the standalone admission boundary opens the package.
    let bundle = Bundle::open(directory.path()).unwrap();
    // Then validated binding sizes and ordered stage names are available.
    assert_eq!(bundle.input_size("frame"), Some(16));
    assert_eq!(bundle.output_size("model"), Some(16));
    assert_eq!(
        bundle.entrypoints().collect::<Vec<_>>(),
        ["prepare", "policy"]
    );
}
