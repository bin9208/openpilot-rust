use openpilot_card::brands::hyundai::stalks::Stalks;
#[test]
fn pinned_carstate_stalk_counts_detents_disappearance_and_u8_rollover_match_native() {
    let data =
        zstd::stream::decode_all(include_bytes!("fixtures/stalks-final.json.zst").as_slice())
            .unwrap();
    let fixture: serde_json::Value = serde_json::from_slice(&data).unwrap();
    let mut state = Stalks {
        left_blinker_stalk_count: 254,
        right_blinker_stalk_count: 254,
        ..Stalks::default()
    };
    for (i, row) in fixture["rows"].as_array().unwrap().iter().enumerate() {
        let sample: Option<[bool; 4]> = serde_json::from_value(row["sample"].clone()).unwrap();
        assert_eq!(
            serde_json::json!(state.update(sample)),
            row["result"],
            "result/{i}"
        );
        assert_eq!(
            serde_json::to_value(&state).unwrap(),
            row["state"],
            "state/{i}"
        );
    }
    assert_eq!(fixture["rows"].as_array().unwrap().len(), 4096);
}
