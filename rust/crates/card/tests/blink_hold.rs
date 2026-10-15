use openpilot_card::brands::hyundai::blink_hold::BlinkHold;
#[test]
fn pinned_lane_crossing_and_requested_hold_match_4096_transitions() {
    let bytes =
        zstd::stream::decode_all(include_bytes!("fixtures/blink-hold-final.json.zst").as_slice())
            .unwrap();
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(rows.len(), 4096);
    for (index, row) in rows.iter().enumerate() {
        let mut state: BlinkHold = serde_json::from_value(row["before"].clone()).unwrap();
        let requested = match row["hold"].as_str().unwrap() {
            "left" => 1,
            "right" => 2,
            _ => 0,
        };
        let actual = state.update(
            u16::try_from(row["desire"].as_u64().unwrap()).unwrap(),
            serde_json::from_value(row["lines"].clone()).unwrap(),
            requested,
        );
        assert_eq!(serde_json::json!(actual), row["result"], "hold/{index}");
        assert_eq!(
            serde_json::to_value(state).unwrap(),
            row["after"],
            "state/{index}"
        );
    }
}

#[test]
fn pinned_cluster_background_prioritizes_active_turn_and_paddle() {
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/cluster-background-final.json")).unwrap();
    assert_eq!(rows.len(), 64);
    for row in rows {
        let input = &row["input"];
        let actual = openpilot_card::brands::hyundai::cluster_hud::background(
            input[0].as_bool().unwrap(),
            input[1].as_bool().unwrap(),
            input[2].as_bool().unwrap(),
            i32::try_from(input[3].as_i64().unwrap()).unwrap(),
            input[4].as_bool().unwrap(),
        );
        assert_eq!(actual, row["result"].as_f64().unwrap());
    }
}
