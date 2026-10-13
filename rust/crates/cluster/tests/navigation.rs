use openpilot_cereal::custom_capnp::carrot_navi_state;
use openpilot_cluster::live::navigation::{
    fresh_carrot_navi, parse_carrot_navi, resolve_navi_speed_limit,
};

#[test]
fn repeated_meta_retains_value_until_exact_expiry_and_session_change_replaces_it() {
    let mut message = capnp::message::Builder::new_default();
    let mut root = message.init_root::<carrot_navi_state::Builder<'_>>();
    root.set_schema_version(1);
    root.set_connected(true);
    root.set_session_id("a");
    let mut speed = root.reborrow().init_speed();
    speed.set_road_limit_valid(true);
    speed.set_road_limit_kph(80);
    let mut meta = speed.init_meta();
    meta.set_present(true);
    meta.set_sequence(u64::MAX);
    meta.set_received_mono_time_nanos(100_000_000_000);
    let first = parse_carrot_navi(root.reborrow_as_reader().into(), 100.0, None).unwrap();
    assert_eq!(first.speed.as_ref().unwrap().road_limit_kph, Some(80));
    assert_eq!(first.speed.as_ref().unwrap().meta.sequence, u64::MAX);
    root.reborrow().get_speed().unwrap().set_road_limit_kph(100);
    let unchanged =
        parse_carrot_navi(root.reborrow_as_reader().into(), 101.0, Some(&first)).unwrap();
    assert_eq!(unchanged.speed.as_ref().unwrap().road_limit_kph, Some(80));
    assert_eq!(
        fresh_carrot_navi(Some(&unchanged), 105.0).1.to_bits(),
        106.0_f64.to_bits()
    );
    assert!(fresh_carrot_navi(Some(&unchanged), 106.0).0.is_none());
    root.set_session_id("b");
    let changed = parse_carrot_navi(root.reborrow_as_reader().into(), 101.0, Some(&first)).unwrap();
    assert_eq!(changed.speed.as_ref().unwrap().road_limit_kph, Some(100));
    assert_eq!(
        resolve_navi_speed_limit(Some(30), Some("v"), Some(&changed)),
        (Some(30), Some("v".into()))
    );
    assert_eq!(
        resolve_navi_speed_limit(Some(30), Some("c"), Some(&changed)),
        (Some(100), Some("n".into()))
    );
}

#[test]
fn invalid_road_limit_clears_base_and_future_timestamp_uses_receipt_clock() {
    let mut message = capnp::message::Builder::new_default();
    let mut root = message.init_root::<carrot_navi_state::Builder<'_>>();
    root.set_schema_version(1);
    root.set_connected(true);
    let mut speed = root.reborrow().init_speed();
    speed.set_road_limit_valid(true);
    speed.set_road_limit_kph(85);
    let mut meta = speed.init_meta();
    meta.set_present(true);
    meta.set_received_mono_time_nanos(161_000_000_000);
    let state = parse_carrot_navi(root.reborrow_as_reader().into(), 100.0, None).unwrap();
    assert_eq!(
        state.speed.as_ref().unwrap().meta.received_mono_s.to_bits(),
        100.0_f64.to_bits()
    );
    assert_eq!(
        resolve_navi_speed_limit(Some(30), Some("c"), Some(&state)),
        (None, None)
    );
    assert!(parse_carrot_navi(root.reborrow_as_reader().into(), 100.0, None).is_some());
    root.set_connected(false);
    assert!(parse_carrot_navi(root.reborrow_as_reader().into(), 100.0, Some(&state)).is_none());
}
