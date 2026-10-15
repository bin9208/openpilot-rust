use openpilot_radarcan::decoder::hyundai::bosch::group_cluster;
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
#[derive(Deserialize)]
struct Case {
    compatible: Vec<u32>,
    pair_cost: Vec<f64>,
    ids: Vec<u32>,
    owner: BTreeMap<u32, u32>,
    max_members: usize,
    result: Value,
}
#[test]
fn complete_link_merges_preserve_source_owner_preference_and_whole_cluster_order() {
    let bytes = zstd::stream::decode_all(
        include_bytes!("fixtures/bosch-cluster-final.json.zst").as_slice(),
    )
    .unwrap();
    let cases: Vec<Case> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(cases.len(), 1297);
    for (i, case) in cases.iter().enumerate() {
        let result = group_cluster::merge(
            &case.compatible,
            &case.pair_cost,
            &case.ids,
            &case.owner,
            case.max_members,
        );
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            case.result,
            "cluster/{i}"
        );
    }
}
