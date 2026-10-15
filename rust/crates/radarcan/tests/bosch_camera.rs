use openpilot_radarcan::decoder::hyundai::bosch::camera::CameraCycleCache;
fn compare(actual: &serde_json::Value, expected: &serde_json::Value, path: &str) {
    match (actual, expected) {
        (serde_json::Value::Number(a), serde_json::Value::Number(b)) => {
            if let (Some(a), Some(b)) = (a.as_u64(), b.as_u64()) {
                assert_eq!(a, b, "{path}");
            } else {
                assert_eq!(
                    a.as_f64().unwrap().to_bits(),
                    b.as_f64().unwrap().to_bits(),
                    "{path}: {a} != {b}"
                );
            }
        }
        (serde_json::Value::Array(a), serde_json::Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (index, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &format!("{path}/{index}"));
            }
        }
        (serde_json::Value::Object(a), serde_json::Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (key, a) in a {
                compare(a, &b[key], &format!("{path}/{key}"));
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}
#[test]
fn pinned_bosch_camera_cache_matches_complete_state_and_causal_snapshots() {
    let data =
        zstd::stream::decode_all(include_bytes!("fixtures/bosch-camera-final.json.zst").as_slice())
            .unwrap();
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&data).unwrap();
    assert_eq!(rows.len(), 3326);
    compare(
        &serde_json::to_value(CameraCycleCache::default()).unwrap(),
        &rows[0]["before"],
        "initial",
    );
    for (index, row) in rows.iter().enumerate() {
        let mut cache: CameraCycleCache = serde_json::from_value(row["before"].clone()).unwrap();
        let input = &row["input"];
        let result = match row["operation"].as_str().unwrap() {
            "ingest" => serde_json::json!(cache.ingest(
                input[0].as_i64().unwrap(),
                u32::try_from(input[1].as_u64().unwrap()).unwrap(),
                &serde_json::from_value::<Vec<u8>>(input[2].clone()).unwrap()
            )),
            "snapshot" => serde_json::to_value(cache.snapshot(input[0].as_i64().unwrap())).unwrap(),
            other => panic!("unexpected {other}"),
        };
        compare(&result, &row["result"], &format!("result/{index}"));
        compare(
            &serde_json::to_value(cache).unwrap(),
            &row["after"],
            &format!("state/{index}"),
        );
    }
}
