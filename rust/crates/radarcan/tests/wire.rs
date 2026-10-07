use capnp::message::{Builder, ReaderOptions};
use openpilot_cereal::log_capnp::event;
use openpilot_radarcan::{data::Data, wire};

fn publication(data: &Data) -> capnp::message::Reader<capnp::serialize::OwnedSegments> {
    capnp::serialize::read_message(
        std::io::Cursor::new(wire::encode(data, false, 71).unwrap()),
        ReaderOptions::new(),
    )
    .unwrap()
}

#[test]
fn input_error_publication_preserves_absent_points_pointer() {
    let mut data = Data::default();
    data.errors.can_error = true;
    let message = publication(&data);
    let root = message.get_root::<event::Reader>().unwrap();
    let event::Which::LiveTracks(tracks) = root.which().unwrap() else {
        panic!("expected tracks")
    };
    assert!(!tracks.unwrap().has_points());
}

#[test]
fn decoder_empty_selection_preserves_present_empty_points_pointer() {
    let data = Data {
        points_present: true,
        ..Default::default()
    };
    let message = publication(&data);
    let root = message.get_root::<event::Reader>().unwrap();
    let event::Which::LiveTracks(tracks) = root.which().unwrap() else {
        panic!("expected tracks")
    };
    assert!(tracks.unwrap().has_points());
}

#[test]
fn can_validity_flag_does_not_filter_source_packets() {
    let mut message = Builder::new_default();
    let mut root = message.init_root::<event::Builder>();
    root.set_valid(false);
    root.set_log_mono_time(101);
    root.init_can(0);
    let packet = wire::can(&capnp::serialize::write_message_to_words(&message)).unwrap();
    assert_eq!((packet.mono_time, packet.frames.len()), (101, 0));
}

#[test]
fn car_state_preserves_batch_identity_and_promotes_schema_float_values() {
    let mut message = Builder::new_default();
    let mut root = message.init_root::<event::Builder>();
    root.set_valid(false);
    let mut state = root.init_car_state();
    state.set_v_ego(12.3);
    state.set_a_ego(-0.2);
    let mut input = state.init_radar_input();
    input.set_first_can_mono_time(20);
    input.set_last_can_mono_time(30);
    input.set_can_packet_count(3);
    input.set_receive_mono_time(31);
    let ego = wire::ego(&capnp::serialize::write_message_to_words(&message)).unwrap();
    assert_eq!(
        (
            ego.first_can_ns,
            ego.last_can_ns,
            ego.packet_count,
            ego.receive_ns,
            ego.v_ego,
            ego.a_ego
        ),
        (20, 30, 3, 31, f64::from(12.3f32), f64::from(-0.2f32))
    );
}

#[test]
fn malformed_wire_bytes_remain_a_typed_fatal_boundary() {
    assert!(matches!(wire::can(&[0]), Err(wire::Error::ByteLength)));
}
