use capnp::message::Builder;
use openpilot_card::xiaoge;
use openpilot_cereal::car_capnp::car_state;

#[test]
fn lane_color_and_stock_blindspot_survive_inclusive_freshness_boundary() {
    let result = xiaoge::parse(br#"{"type":"xiaogeVision","version":1,"lane":{"leftLine":1,"rightLine":-1,"valid":true,"receivedMonoTimeNanos":1},"blindspot":{"left":false,"right":true,"valid":true,"receivedMonoTimeNanos":2500000001}}"#).unwrap();
    let mut message = Builder::new_default();
    let mut cs = message.init_root::<car_state::Builder>();
    cs.set_left_lane_line(20);
    cs.set_right_lane_line(10);
    cs.set_left_blindspot(true);
    assert!(xiaoge::apply(cs, Some(&result), 4000000001));
    let cs = message.get_root_as_reader::<car_state::Reader>().unwrap();
    assert_eq!(cs.get_left_lane_line(), 21);
    assert_eq!(cs.get_right_lane_line(), 10);
    assert!(cs.get_left_blindspot());
    assert!(cs.get_right_blindspot());
}

#[test]
fn payload_rejects_boolean_lane_and_accepts_source_integer_domain() {
    let bytes = br#"{"type":"xiaogeVision","version":true,"lane":{"leftLine":false,"rightLine":1,"valid":true,"receivedMonoTimeNanos":1},"blindspot":{"left":false,"right":true,"valid":true,"receivedMonoTimeNanos":1}}"#;
    assert!(xiaoge::parse(bytes).is_err());
    let text = std::str::from_utf8(bytes)
        .unwrap()
        .replace("\"leftLine\":false", "\"leftLine\":0")
        .replace(
            "\"receivedMonoTimeNanos\":1",
            "\"receivedMonoTimeNanos\":18446744073709551616",
        );
    let result = xiaoge::parse(text.as_bytes()).unwrap();
    let mut message = Builder::new_default();
    let cs = message.init_root::<car_state::Builder>();
    assert!(!xiaoge::apply(cs, Some(&result), u64::MAX));
}

#[test]
fn final_pinned_payloads_match_independent_side_freshness_and_legacy_fallback() {
    let bytes =
        zstd::stream::decode_all(include_bytes!("fixtures/xiaoge-final.json.zst").as_slice())
            .unwrap();
    let fixture: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let rows = fixture["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 573);
    for (index, row) in rows.iter().enumerate() {
        let input = &row["input"];
        let bytes: Vec<u8> = serde_json::from_value(input["bytes"].clone()).unwrap();
        let result = xiaoge::parse(&bytes);
        let mut message = capnp::message::Builder::new_default();
        let mut state = message.init_root::<openpilot_cereal::car_capnp::car_state::Builder>();
        let lanes: [i16; 2] = serde_json::from_value(input["lanes"].clone()).unwrap();
        let spots: [bool; 2] = serde_json::from_value(input["spots"].clone()).unwrap();
        state.set_left_lane_line(lanes[0]);
        state.set_right_lane_line(lanes[1]);
        state.set_left_blindspot(spots[0]);
        state.set_right_blindspot(spots[1]);
        let applied = xiaoge::apply(
            state.reborrow(),
            result.as_ref().ok(),
            input["now"].as_u64().unwrap(),
        );
        let state = state.reborrow_as_reader();
        let actual = serde_json::json!({"accepted":result.is_ok(),"applied":applied,"lanes":[state.get_left_lane_line(),state.get_right_lane_line()],"spots":[state.get_left_blindspot(),state.get_right_blindspot()]});
        assert_eq!(actual, row["expected"], "source payload {index}");
    }
}
