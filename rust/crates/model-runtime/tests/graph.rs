use openpilot_model_runtime::Graph;
use serde_json::{json, Value};

fn manifest() -> Value {
    json!({
        "version": 1, "backend": "cpu-clang", "arch": std::env::consts::ARCH,
        "weights_sha256": "00".repeat(32), "library_sha256": "11".repeat(32),
        "allocations": [{"bytes": 64, "weight_offset": 0}],
        "views": [{"allocation": 0, "offset": 0, "bytes": 32},
                  {"allocation": 0, "offset": 16, "bytes": 32}],
        "inputs": [{"name": "input", "view": 0}],
        "outputs": [{"name": "output", "view": 1}],
        "kernels": [{"buffers": 2, "scalars": 1}],
        "calls": [{"kernel": 0, "views": [1, 0], "scalars": [0],
                   "workers": 2, "core_id": 0}]
    })
}

fn parse(value: &Value) -> bool {
    Graph::parse(&serde_json::to_vec(value).unwrap()).is_ok()
}

#[test]
fn overlapping_views_are_valid_and_share_one_allocation() {
    let graph = Graph::parse(&serde_json::to_vec(&manifest()).unwrap()).unwrap();
    assert_eq!(graph.allocation_bytes(), 64);
}

#[test]
fn rejects_invalid_ranges_and_indices() {
    for (pointer, replacement) in [
        ("/views/1/offset", json!(33)),
        ("/views/1/offset", json!(u64::MAX)),
        ("/views/1/allocation", json!(1)),
        ("/views/1/bytes", json!(0)),
        ("/inputs/0/view", json!(2)),
        ("/calls/0/kernel", json!(1)),
        ("/calls/0/views/0", json!(2)),
        ("/allocations/0/weight_offset", json!(u64::MAX)),
        (
            "/allocations/0/weight_offset",
            json!(4_u64 * 1024 * 1024 * 1024),
        ),
    ] {
        let mut value = manifest();
        *value.pointer_mut(pointer).unwrap() = replacement;
        assert!(!parse(&value), "accepted {pointer}: {value}");
    }
}

#[test]
fn rejects_incompatible_contract_and_resource_limits() {
    for (pointer, replacement) in [
        ("/version", json!(2)),
        ("/backend", json!("qcom")),
        ("/arch", json!("unknown")),
        ("/allocations/0/bytes", json!(0)),
        ("/allocations/0/bytes", json!(5_u64 * 1024 * 1024 * 1024)),
        ("/weights_sha256", json!("bad")),
        ("/library_sha256", json!("zz".repeat(32))),
        ("/inputs/0/name", json!("")),
        ("/calls/0/views", json!([0])),
        ("/calls/0/scalars", json!([])),
        ("/calls/0/workers", json!(0)),
        ("/calls/0/workers", json!(1025)),
        ("/calls/0/core_id", json!(1)),
        ("/calls/0/core_id", Value::Null),
    ] {
        let mut value = manifest();
        *value.pointer_mut(pointer).unwrap() = replacement;
        assert!(!parse(&value), "accepted {pointer}: {value}");
    }
}

#[test]
fn rejects_duplicate_names_and_unknown_fields() {
    let mut value = manifest();
    value["inputs"] = json!([{"name":"input","view":0}, {"name":"input","view":1}]);
    assert!(!parse(&value));
    let mut value = manifest();
    value["calls"][0]["typo"] = json!(1);
    assert!(!parse(&value));
}

#[test]
fn validates_cumulative_memory_not_only_each_allocation() {
    let mut value = manifest();
    value["allocations"] = json!([
        {"bytes": 3_u64 * 1024 * 1024 * 1024, "weight_offset": null},
        {"bytes": 3_u64 * 1024 * 1024 * 1024, "weight_offset": null}
    ]);
    assert!(!parse(&value));
}

#[test]
fn named_entries_require_a_complete_ordered_partition() {
    let mut value = manifest();
    value["version"] = json!(2);
    value["entrypoints"] = json!([
        {"name":"prepare", "start":0, "end":0},
        {"name":"policy", "start":0, "end":1}
    ]);
    assert!(parse(&value));
    for entries in [
        json!([]),
        json!([{"name":"policy", "start":1, "end":1}]),
        json!([{"name":"policy", "start":0, "end":2}]),
        json!([{"name":"policy", "start":0, "end":0}]),
        json!([{"name":"", "start":0, "end":1}]),
        json!([{"name":"a", "start":0, "end":1}, {"name":"a", "start":1, "end":1}]),
        json!([{"name":"a", "start":0, "end":1}, {"name":"b", "start":0, "end":1}]),
    ] {
        let mut bad = value.clone();
        bad["entrypoints"] = entries;
        assert!(!parse(&bad), "accepted {bad}");
    }
    value["version"] = json!(1);
    assert!(!parse(&value));
}
