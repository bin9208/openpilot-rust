use openpilot_radarcan::decoder::hyundai::bosch::graph::{self, Edges};
use serde::Deserialize;
use serde_json::{json, Value};
#[derive(Deserialize)]
struct Case {
    n: usize,
    m: usize,
    rows: Edges,
    columns: Edges,
    cost: f64,
    result: Value,
}
#[test]
fn optional_assignment_uniqueness_ambiguity_and_component_stats_match_source() {
    let bytes =
        zstd::stream::decode_all(include_bytes!("fixtures/bosch-graph-final.json.zst").as_slice())
            .unwrap();
    let rows: Vec<Case> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(rows.len(), 3072);
    for (i, case) in rows.iter().enumerate() {
        let result = graph::solve(case.n, case.m, &case.rows, &case.columns, case.cost).unwrap();
        assert_eq!(
            json!([
                result.assignment,
                result.components,
                result.largest,
                result.certified,
                result.fallback,
                result.ambiguous
            ]),
            case.result,
            "graph/{i}"
        );
    }
}

#[test]
fn physical_optional_score_matching_refuses_nonunique_or_wide_components_like_source() {
    let bytes = zstd::stream::decode_all(
        include_bytes!("fixtures/bosch-physical-graph-final.json.zst").as_slice(),
    )
    .unwrap();
    let rows: Vec<Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(rows.len(), 2048);
    for (i, case) in rows.iter().enumerate() {
        let rows: Edges = serde_json::from_value(case["rows"].clone()).unwrap();
        let columns: Edges = serde_json::from_value(case["columns"].clone()).unwrap();
        assert_eq!(
            serde_json::to_value(graph::physical(&rows, &columns)).unwrap(),
            case["result"],
            "physical graph/{i}"
        );
    }
}
