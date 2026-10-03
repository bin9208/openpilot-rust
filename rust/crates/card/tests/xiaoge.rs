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
