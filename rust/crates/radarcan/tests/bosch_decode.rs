use openpilot_radarcan::decoder::hyundai::bosch::raw::decode_frame;
#[test]
fn pinned_raw_detection_words_errors_and_inactive_halves_match_4096_frames() {
    let data =
        zstd::stream::decode_all(include_bytes!("fixtures/bosch-decode-final.json.zst").as_slice())
            .unwrap();
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&data).unwrap();
    assert_eq!(rows.len(), 4096);
    for (index, row) in rows.iter().enumerate() {
        let payload: Vec<u8> = serde_json::from_value(row["payload"].clone()).unwrap();
        let result = match decode_frame(
            row["timestamp"].as_i64().unwrap(),
            u32::try_from(row["address"].as_u64().unwrap()).unwrap(),
            &payload,
        ) {
            Ok((objects, unsupported)) => {
                serde_json::json!({"objects":objects,"unsupported":unsupported})
            }
            Err(error) => serde_json::json!({"error":error}),
        };
        assert_eq!(result, row["result"], "frame/{index}");
    }
}

#[test]
fn pinned_raw_constructor_validation_precedence_and_properties_match() {
    use openpilot_radarcan::decoder::hyundai::bosch::raw::RawDetection;
    fn float(value: &serde_json::Value) -> f64 {
        value
            .as_f64()
            .unwrap_or_else(|| match value.as_str().unwrap() {
                "nan" => f64::NAN,
                "inf" => f64::INFINITY,
                "-inf" => f64::NEG_INFINITY,
                _ => panic!("float"),
            })
    }
    let bytes = zstd::stream::decode_all(
        include_bytes!("fixtures/bosch-raw-constructor-final.json.zst").as_slice(),
    )
    .unwrap();
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(rows.len(), 1440);
    for (index, row) in rows.iter().enumerate() {
        let input = &row["input"];
        let result = match RawDetection::new(
            input[0].as_i64().unwrap(),
            usize::try_from(input[1].as_u64().unwrap()).unwrap(),
            float(&input[2]),
            float(&input[3]),
            float(&input[4]),
            u32::try_from(input[5].as_u64().unwrap()).unwrap(),
        ) {
            Ok(object) => {
                serde_json::json!({"address":object.address(),"half":object.half(),"object":object})
            }
            Err(error) => serde_json::json!({"error":error}),
        };
        assert_eq!(result, row["result"], "constructor/{index}");
    }
}
