//! Wire compatibility for the additive 2026-10-15 source schema import.
//! The one-word null root represents a message from before the added fields.
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use openpilot_cereal::{car_capnp, custom_capnp, log_capnp};

const OLD_EMPTY_ROOT: [u8; 16] = [0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

#[test]
fn old_dm_messages_preserve_neutral_timeout_factors() {
    let message =
        serialize::read_message(&mut OLD_EMPTY_ROOT.as_slice(), ReaderOptions::new()).unwrap();
    let dm = message
        .get_root::<log_capnp::driver_monitoring_state::Reader>()
        .unwrap();
    assert!(!dm.get_camera_unavailable());
    assert!(!dm.get_dm2_experimental());
    assert_eq!(dm.get_dm2_wheel_timeout_factor(), 1.0);
    assert_eq!(dm.get_dm2_vision_timeout_factor(), 1.0);
    assert_eq!(dm.get_dm2_strict_time_remaining(), 0.0);
    assert_eq!(dm.get_dm2_forward_attention_score(), 0.0);
    assert!(!dm.get_dm2_forward_recovery());
    assert_eq!(dm.get_dm2_interaction_credit(), 0.0);
    assert_eq!(dm.get_dm2_interaction_grace_remaining(), 0.0);
    assert!(!dm.get_dm2_disabled());
}

#[test]
fn car_sources_and_stalk_edges_roundtrip_without_merging_old_blindspots() {
    let mut message = Builder::new_default();
    {
        let mut car = message.init_root::<car_capnp::car_state::Builder>();
        car.set_left_blindspot(true);
        car.set_left_blindspot_oem(false);
        car.set_right_blindspot_oem(true);
        car.set_left_blindspot_onnx(true);
        car.set_right_blindspot_onnx(false);
        car.set_blindspot_split_sources_valid(true);
        car.set_left_blinker_stalk_count(255);
        car.set_right_blinker_stalk_count(17);
        car.set_blinker_lever(2);
        car.set_blinker_lamp(1);
        let mut touch = car.init_steering_touch();
        touch.set_available(true);
        touch.set_valid(true);
        touch.set_touched(true);
        touch.set_sample_mono_time(9_000_000_001);
        touch.set_raw_status(7);
        touch.set_raw_touch1(128);
        touch.set_raw_touch2(255);
    }
    let bytes = serialize::write_message_to_words(&message);
    let decoded = serialize::read_message(&mut bytes.as_slice(), ReaderOptions::new()).unwrap();
    let car = decoded.get_root::<car_capnp::car_state::Reader>().unwrap();
    assert!(car.get_left_blindspot());
    assert!(!car.get_left_blindspot_oem());
    assert!(car.get_right_blindspot_oem());
    assert!(car.get_left_blindspot_onnx());
    assert!(!car.get_right_blindspot_onnx());
    assert!(car.get_blindspot_split_sources_valid());
    assert_eq!(car.get_left_blinker_stalk_count(), 255);
    assert_eq!(car.get_right_blinker_stalk_count(), 17);
    assert_eq!(car.get_blinker_lever(), 2);
    assert_eq!(car.get_blinker_lamp(), 1);
    let touch = car.get_steering_touch().unwrap();
    assert!(touch.get_available() && touch.get_valid() && touch.get_touched());
    assert_eq!(touch.get_sample_mono_time(), 9_000_000_001);
    assert_eq!(
        (
            touch.get_raw_status(),
            touch.get_raw_touch1(),
            touch.get_raw_touch2()
        ),
        (7, 128, 255)
    );
    let old =
        serialize::read_message(&mut OLD_EMPTY_ROOT.as_slice(), ReaderOptions::new()).unwrap();
    let car = old.get_root::<car_capnp::car_state::Reader>().unwrap();
    assert!(!car.get_blindspot_split_sources_valid());
    assert!(!car.get_steering_touch().unwrap().get_available());
    assert_eq!(car.get_left_blinker_stalk_count(), 0);
}

#[test]
fn highway_signal_status_roundtrips_and_old_messages_do_not_assert_evidence() {
    let mut message = Builder::new_default();
    {
        let mut plan = message.init_root::<log_capnp::longitudinal_plan::Builder>();
        plan.set_signal_detection_suspended(true);
        plan.set_signal_observation_color(3);
        plan.set_signal_assist_action(2);
    }
    let bytes = serialize::write_message_to_words(&message);
    let decoded = serialize::read_message(&mut bytes.as_slice(), ReaderOptions::new()).unwrap();
    let plan = decoded
        .get_root::<log_capnp::longitudinal_plan::Reader>()
        .unwrap();
    assert!(plan.get_signal_detection_suspended());
    assert_eq!(plan.get_signal_observation_color(), 3);
    assert_eq!(plan.get_signal_assist_action(), 2);
    let old =
        serialize::read_message(&mut OLD_EMPTY_ROOT.as_slice(), ReaderOptions::new()).unwrap();
    let plan = old
        .get_root::<log_capnp::longitudinal_plan::Reader>()
        .unwrap();
    assert!(!plan.get_signal_detection_suspended());
    assert_eq!(plan.get_signal_observation_color(), 0);
    assert_eq!(plan.get_signal_assist_action(), 0);
}

#[test]
fn wake_is_separate_from_ignition_on_wire() {
    let mut message = Builder::new_default();
    {
        let mut panda = message.init_root::<log_capnp::panda_state::Builder>();
        panda.set_wake_on_can(true);
        panda.set_ignition_can(false);
    }
    let bytes = serialize::write_message_to_words(&message);
    let decoded = serialize::read_message(&mut bytes.as_slice(), ReaderOptions::new()).unwrap();
    let panda = decoded
        .get_root::<log_capnp::panda_state::Reader>()
        .unwrap();
    assert!(panda.get_wake_on_can());
    assert!(!panda.get_ignition_can());
    let old =
        serialize::read_message(&mut OLD_EMPTY_ROOT.as_slice(), ReaderOptions::new()).unwrap();
    assert!(!old
        .get_root::<log_capnp::panda_state::Reader>()
        .unwrap()
        .get_wake_on_can());
}

#[test]
fn missing_road_category_is_distinct_from_explicit_zero() {
    let mut message = Builder::new_default();
    {
        let mut lane = message.init_root::<custom_capnp::carrot_navi_state::lane::Builder>();
        lane.set_road_category(0);
        lane.set_road_category_valid(true);
    }
    let bytes = serialize::write_message_to_words(&message);
    let decoded = serialize::read_message(&mut bytes.as_slice(), ReaderOptions::new()).unwrap();
    let lane = decoded
        .get_root::<custom_capnp::carrot_navi_state::lane::Reader>()
        .unwrap();
    assert_eq!(lane.get_road_category(), 0);
    assert!(lane.get_road_category_valid());
    let old =
        serialize::read_message(&mut OLD_EMPTY_ROOT.as_slice(), ReaderOptions::new()).unwrap();
    assert!(!old
        .get_root::<custom_capnp::carrot_navi_state::lane::Reader>()
        .unwrap()
        .get_road_category_valid());
}

#[test]
fn native_event_ordinals_never_relabel_recorded_jetlink_loss() {
    // One segment, root struct with one data word, EventName at data byte 0.
    // This literal was recorded under the existing native schema, where 126 is JetlinkLost.
    let legacy = [
        0u8, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 126, 0, 0, 0, 0, 0, 0, 0,
    ];
    let decoded = serialize::read_message(&mut legacy.as_slice(), ReaderOptions::new()).unwrap();
    assert_eq!(
        decoded
            .get_root::<log_capnp::onroad_event::Reader>()
            .unwrap()
            .get_name()
            .unwrap(),
        log_capnp::onroad_event::EventName::JetlinkLost
    );
    let mut message = Builder::new_default();
    message
        .init_root::<log_capnp::onroad_event::Builder>()
        .set_name(log_capnp::onroad_event::EventName::DriverMonitorFallback);
    let bytes = serialize::write_message_to_words(&message);
    // Native allocation 130 deliberately differs from upstream source's conflicting 126.
    assert_eq!(&bytes[16..18], &[130, 0]);
    let decoded = serialize::read_message(&mut bytes.as_slice(), ReaderOptions::new()).unwrap();
    assert_eq!(
        decoded
            .get_root::<log_capnp::onroad_event::Reader>()
            .unwrap()
            .get_name()
            .unwrap(),
        log_capnp::onroad_event::EventName::DriverMonitorFallback
    );
}

#[test]
fn display_radar_is_independent_from_control_points_with_explicit_capability() {
    let mut message = Builder::new_default();
    {
        let mut radar = message.init_root::<car_capnp::radar_data::Builder>();
        radar.set_has_display_data(true);
        radar.reborrow().init_points(1).get(0).set_track_id(11);
        radar.init_display_points(1).get(0).set_track_id(22);
    }
    let bytes = serialize::write_message_to_words(&message);
    let decoded = serialize::read_message(&mut bytes.as_slice(), ReaderOptions::new()).unwrap();
    let radar = decoded.get_root::<car_capnp::radar_data::Reader>().unwrap();
    assert!(radar.get_has_display_data());
    assert_eq!(radar.get_points().unwrap().get(0).get_track_id(), 11);
    assert_eq!(
        radar.get_display_points().unwrap().get(0).get_track_id(),
        22
    );
    let old =
        serialize::read_message(&mut OLD_EMPTY_ROOT.as_slice(), ReaderOptions::new()).unwrap();
    let radar = old.get_root::<car_capnp::radar_data::Reader>().unwrap();
    assert!(!radar.get_has_display_data());
    assert_eq!(radar.get_display_points().unwrap().len(), 0);
}

#[test]
fn dm_experimental_values_override_nonzero_defaults_on_wire() {
    let mut message = Builder::new_default();
    {
        let mut dm = message.init_root::<log_capnp::driver_monitoring_state::Builder>();
        dm.set_camera_unavailable(true);
        dm.set_dm2_experimental(true);
        dm.set_dm2_wheel_timeout_factor(0.0);
        dm.set_dm2_vision_timeout_factor(1.5);
        dm.set_dm2_strict_time_remaining(7.0);
        dm.set_dm2_forward_attention_score(0.5);
        dm.set_dm2_forward_recovery(true);
        dm.set_dm2_interaction_credit(2.0);
        dm.set_dm2_interaction_grace_remaining(3.0);
        dm.set_dm2_disabled(true);
    }
    let bytes = serialize::write_message_to_words(&message);
    let decoded = serialize::read_message(&mut bytes.as_slice(), ReaderOptions::new()).unwrap();
    let dm = decoded
        .get_root::<log_capnp::driver_monitoring_state::Reader>()
        .unwrap();
    assert!(dm.get_camera_unavailable() && dm.get_dm2_experimental() && dm.get_dm2_disabled());
    assert_eq!(dm.get_dm2_wheel_timeout_factor(), 0.0);
    assert_eq!(dm.get_dm2_vision_timeout_factor(), 1.5);
    assert_eq!(dm.get_dm2_strict_time_remaining(), 7.0);
    assert_eq!(dm.get_dm2_forward_attention_score(), 0.5);
    assert!(dm.get_dm2_forward_recovery());
    assert_eq!(dm.get_dm2_interaction_credit(), 2.0);
    assert_eq!(dm.get_dm2_interaction_grace_remaining(), 3.0);
}

#[test]
fn added_metadata_and_freshness_survive_serialization() {
    let mut model_message = Builder::new_default();
    {
        let mut meta = model_message
            .init_root::<log_capnp::model_data_v2::Builder>()
            .init_meta();
        meta.set_lane_change_blinker_hold(log_capnp::LaneChangeDirection::Right);
        meta.set_lever_turn(true);
    }
    let bytes = serialize::write_message_to_words(&model_message);
    let decoded = serialize::read_message(&mut bytes.as_slice(), ReaderOptions::new()).unwrap();
    let meta = decoded
        .get_root::<log_capnp::model_data_v2::Reader>()
        .unwrap()
        .get_meta()
        .unwrap();
    assert_eq!(
        meta.get_lane_change_blinker_hold().unwrap(),
        log_capnp::LaneChangeDirection::Right
    );
    assert!(meta.get_lever_turn());
    let mut process_message = Builder::new_default();
    process_message
        .init_root::<log_capnp::proc_log::process::Builder>()
        .set_mem_pss_mono_time(8_765_432_101);
    let bytes = serialize::write_message_to_words(&process_message);
    let decoded = serialize::read_message(&mut bytes.as_slice(), ReaderOptions::new()).unwrap();
    assert_eq!(
        decoded
            .get_root::<log_capnp::proc_log::process::Reader>()
            .unwrap()
            .get_mem_pss_mono_time(),
        8_765_432_101
    );
    let old =
        serialize::read_message(&mut OLD_EMPTY_ROOT.as_slice(), ReaderOptions::new()).unwrap();
    assert_eq!(
        old.get_root::<log_capnp::proc_log::process::Reader>()
            .unwrap()
            .get_mem_pss_mono_time(),
        0
    );
}

#[test]
fn new_reserved_payload_roundtrips_without_reusing_old_union_arm() {
    let mut message = Builder::new_default();
    message
        .init_root::<log_capnp::event::Builder>()
        .set_custom_reserved_raw_data1(b"signal-observer");
    let bytes = serialize::write_message_to_words(&message);
    let decoded = serialize::read_message(&mut bytes.as_slice(), ReaderOptions::new()).unwrap();
    match decoded
        .get_root::<log_capnp::event::Reader>()
        .unwrap()
        .which()
        .unwrap()
    {
        log_capnp::event::CustomReservedRawData1(payload) => {
            assert_eq!(payload.unwrap(), b"signal-observer")
        }
        _ => panic!("reserved payload changed union arm"),
    }
}
