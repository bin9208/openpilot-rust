use openpilot_cereal::log_capnp::event;
use openpilot_dmonitoringd::controller::{Controller, TOPICS};
use openpilot_messaging::state::{Options, Poll, State};
use openpilot_params::Params;

fn frame(id: u32, valid: bool, phone: bool) -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    let mut root = message.init_root::<event::Builder>();
    root.set_valid(valid);
    let mut driver = root.init_driver_state_v2();
    driver.set_frame_id(id);
    let mut data = driver.init_left_driver_data();
    data.set_face_prob(1.);
    data.set_phone_prob(f32::from(phone));
    data.set_face_orientation(&[0., 0., 0.][..]).unwrap();
    data.set_face_orientation_std(&[0., 0., 0.][..]).unwrap();
    data.set_face_position(&[0., 0.][..]).unwrap();
    data.set_face_position_std(&[0., 0.][..]).unwrap();
    capnp::serialize::write_message_to_words(&message)
}

fn state(bytes: &[u8]) -> (bool, bool, bool) {
    let message = capnp::serialize::read_message(
        std::io::Cursor::new(bytes),
        capnp::message::ReaderOptions::new(),
    )
    .unwrap();
    let root = message.get_root::<event::Reader>().unwrap();
    let event::DriverMonitoringState(data) = root.which().unwrap() else {
        panic!("expected monitoring state");
    };
    let data = data.unwrap();
    (
        root.get_valid(),
        data.get_always_on(),
        data.get_vision_policy_state().unwrap().get_is_distracted(),
    )
}

#[test]
fn invalid_inputs_hold_state_and_live_toggles_take_effect_only_after_publication() {
    let directory = tempfile::tempdir().unwrap();
    let params = Params::open(directory.path(), "d").unwrap();
    let mut controller = Controller::new(&params).unwrap();
    let mut subscriptions = State::new(
        TOPICS,
        Options {
            poll: Poll::One("driverStateV2".into()),
            simulation: true,
            ..Options::default()
        },
    )
    .unwrap();
    assert!(controller.prepare(&subscriptions).unwrap().is_none());
    params.put_bool("AlwaysOnDM", true).unwrap();
    params.put_bool("IsDriverViewEnabled", true).unwrap();
    subscriptions.update(1., &[frame(1, false, true)]).unwrap();
    let output = controller.prepare(&subscriptions).unwrap().unwrap();
    assert_eq!(state(&output.bytes), (false, false, false));
    controller.after_publish(output.frame_id, &params).unwrap();
    subscriptions.update(1.05, &[frame(2, true, true)]).unwrap();
    let output = controller.prepare(&subscriptions).unwrap().unwrap();
    assert_eq!(state(&output.bytes), (false, true, true));
    controller.after_publish(output.frame_id, &params).unwrap();
    subscriptions
        .update(1.1, &[frame(3, false, false)])
        .unwrap();
    let output = controller.prepare(&subscriptions).unwrap().unwrap();
    assert_eq!(state(&output.bytes), (false, true, true));
}
