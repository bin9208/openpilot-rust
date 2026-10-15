use openpilot_radarcan::decoder::toyota::ars510::transport::StrictId80RecordAssembler;
#[test]
fn all_pinned_ars510_receiver_tests_match_35192_strict_transport_calls() {
    let data = zstd::stream::decode_all(
        include_bytes!("fixtures/ars510-strict-final.json.zst").as_slice(),
    )
    .unwrap();
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&data).unwrap();
    assert_eq!(rows.len(), 35192);
    for (index, row) in rows.iter().enumerate() {
        let mut assembler: StrictId80RecordAssembler =
            serde_json::from_value(row["before"].clone()).unwrap();
        let time = row["time"]
            .as_f64()
            .unwrap_or_else(|| match row["time"].as_str().unwrap() {
                "nan" => f64::NAN,
                "inf" => f64::INFINITY,
                "-inf" => f64::NEG_INFINITY,
                _ => panic!("time"),
            });
        let payload: Vec<u8> = serde_json::from_value(row["payload"].clone()).unwrap();
        assert_eq!(
            serde_json::to_value(assembler.push(time, &payload)).unwrap(),
            row["result"],
            "record/{index}"
        );
        assert_eq!(
            serde_json::to_value(assembler).unwrap(),
            row["after"],
            "state/{index}"
        );
    }
}
