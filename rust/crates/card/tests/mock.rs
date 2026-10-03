use openpilot_card::{brands::mock::Mock, core::Vehicle};
use openpilot_cereal::{car_capnp::car_state, log_capnp::event};

fn gps(external: bool, speed: f32) -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    let root = message.init_root::<event::Builder>();
    if external {
        root.init_gps_location_external().set_speed(speed);
    } else {
        root.init_gps_location().set_speed(speed);
    }
    capnp::serialize::write_message_to_words(&message)
}

#[test]
fn mock_prefers_external_after_second_receiver_frame_without_rewriting_cluster_speed() {
    let mut vehicle = Mock::new().unwrap();
    vehicle
        .update_gps(1., &[gps(false, 3.), gps(true, 7.)])
        .unwrap();
    let result = vehicle.update(&[], 1).unwrap();
    let state = result.get_root_as_reader::<car_state::Reader>().unwrap();
    assert_eq!(state.get_v_ego(), 3.);
    assert_eq!(state.get_v_ego_cluster(), 0.);
    assert!(state.get_can_valid());
    vehicle.update_gps(1.01, &[]).unwrap();
    vehicle.update_gps(1.02, &[gps(true, 11.)]).unwrap();
    let result = vehicle.update(&[], 2).unwrap();
    assert_eq!(
        result
            .get_root_as_reader::<car_state::Reader>()
            .unwrap()
            .get_v_ego_raw(),
        11.
    );
    vehicle.update_gps(5., &[gps(false, 15.)]).unwrap();
    let result = vehicle.update(&[], 3).unwrap();
    assert_eq!(
        result
            .get_root_as_reader::<car_state::Reader>()
            .unwrap()
            .get_v_ego(),
        11.
    );
}
