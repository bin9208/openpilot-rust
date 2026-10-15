use openpilot_radarcan::decoder::hyundai::bosch::{
    camera::CameraObject,
    camera_association::{associate, Measurement},
};
use serde_json::Value;
#[test]
fn a0_camera_association_matches_original_source() {
    let bytes = zstd::stream::decode_all(
        include_bytes!("fixtures/bosch-association-final.json.zst").as_slice(),
    )
    .unwrap();
    let rows: Vec<Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(rows.len(), 4096);
    for (i, row) in rows.iter().enumerate() {
        let input = &row["input"];
        let measurement: Measurement = serde_json::from_value(input["obj"].clone()).unwrap();
        let cameras: Vec<CameraObject> = serde_json::from_value(input["cameras"].clone()).unwrap();
        let value = associate(
            measurement,
            &cameras,
            input["count"].as_u64().unwrap() as usize,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(value).unwrap(),
            row["result"],
            "association/{i}"
        );
    }
}

#[test]
fn a0_original_regression_calls_match_source() {
    use std::io::{BufRead, BufReader};
    let decoder = zstd::stream::read::Decoder::new(
        include_bytes!("fixtures/bosch-association-regression.jsonl.zst").as_slice(),
    )
    .unwrap();
    let mut count = 0;
    for (i, line) in BufReader::new(decoder).lines().enumerate() {
        let row: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let input = &row["input"];
        let measurement: Measurement = serde_json::from_value(input["obj"].clone()).unwrap();
        let cameras: Vec<CameraObject> =
            serde_json::from_value(input["camera_objects"].clone()).unwrap();
        let value = associate(
            measurement,
            &cameras,
            input["count"].as_u64().unwrap() as usize,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(value).unwrap(),
            row["result"],
            "source-association/{i}"
        );
        count += 1;
    }
    assert_eq!(count, 108);
}
