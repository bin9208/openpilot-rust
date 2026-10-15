use openpilot_radarcan::decoder::hyundai::bosch::{
    physical::{self, Object},
    tracking::Track,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
#[derive(Deserialize)]
struct Case {
    candidates: Vec<Track>,
    raw_index: BTreeMap<u32, usize>,
    vision: Vec<bool>,
    oem: Option<usize>,
    prior: Option<Object>,
    predicted: Option<(f64, f64)>,
    result: Value,
    object: Object,
    published: Value,
}
#[test]
fn representative_keeps_whole_member_and_separates_identity_from_publication() {
    let bytes = zstd::stream::decode_all(
        include_bytes!("fixtures/bosch-representative-final.json.zst").as_slice(),
    )
    .unwrap();
    let cases: Vec<Case> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(cases.len(), 4096);
    for (i, case) in cases.iter().enumerate() {
        let result = physical::representative(
            &case.candidates,
            &case.raw_index,
            &case.vision,
            case.oem,
            case.prior.as_ref().map(|p| (p, case.predicted.unwrap())),
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            case.result,
            "whole member/{i}"
        );
        assert_eq!(
            serde_json::to_value(case.object.published()).unwrap(),
            case.published,
            "publication/{i}"
        );
    }
}
