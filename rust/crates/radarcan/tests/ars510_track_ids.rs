use openpilot_radarcan::decoder::toyota::ars510::tracks::NativeTrackIdAssigner;
#[test]
fn pinned_ars510_allocation_identity_preserves_state_errors_and_idempotence() {
    let data = zstd::stream::decode_all(
        include_bytes!("fixtures/ars510-track-ids-final.json.zst").as_slice(),
    )
    .unwrap();
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&data).unwrap();
    assert_eq!(rows.len(), 380);
    assert_eq!(
        serde_json::to_value(NativeTrackIdAssigner::default()).unwrap(),
        rows[0]["before"]
    );
    for (index, row) in rows.iter().enumerate() {
        let mut assigner: NativeTrackIdAssigner =
            serde_json::from_value(row["before"].clone()).unwrap();
        let input = &row["input"];
        let time = input[0]
            .as_f64()
            .unwrap_or_else(|| match input[0].as_str().unwrap() {
                "nan" => f64::NAN,
                "inf" => f64::INFINITY,
                _ => panic!("time"),
            });
        let result = assigner.update(time, input[1].as_i64().unwrap(), input[2].as_i64().unwrap());
        if let Some(error) = row.get("error") {
            assert_eq!(
                result.unwrap_err(),
                error.as_str().unwrap(),
                "error/{index}"
            );
        } else {
            assert_eq!(
                result.unwrap(),
                row["result"].as_u64().unwrap(),
                "identity/{index}"
            );
        }
        assert_eq!(
            serde_json::to_value(assigner).unwrap(),
            row["after"],
            "state/{index}"
        );
    }
}
