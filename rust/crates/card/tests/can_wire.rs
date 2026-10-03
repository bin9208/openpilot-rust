use capnp::message::Builder;
use openpilot_can::Frame;
use openpilot_card::can_wire;
use openpilot_cereal::log_capnp::event;

#[test]
fn incoming_batches_retain_bus_bytes_and_monotonic_metadata() {
    let frames = vec![Frame {
        address: 123,
        data: vec![1, 2, 3],
        bus: 4,
    }];
    let mut message = Builder::new_default();
    let mut root = message.init_root::<event::Builder>();
    root.set_log_mono_time(456);
    let mut can = root.init_can(1).get(0);
    can.set_address(123);
    can.set_dat(&[1, 2, 3]);
    can.set_src(4);
    let bytes = capnp::serialize::write_message_to_words(&message);
    let packet = can_wire::decode(&bytes).unwrap();
    assert_eq!(packet.mono_time, 456);
    assert_eq!(packet.frames.len(), frames.len());
    assert_eq!(packet.frames[0].address, frames[0].address);
    assert_eq!(packet.frames[0].bus, frames[0].bus);
    assert_eq!(packet.frames[0].data, frames[0].data);
    let encoded = can_wire::sendcan(&frames, false, 789).unwrap();
    let msg = capnp::serialize::read_message(
        std::io::Cursor::new(encoded),
        capnp::message::ReaderOptions::new(),
    )
    .unwrap();
    let root = msg.get_root::<event::Reader>().unwrap();
    assert!(!root.get_valid());
    assert_eq!(root.get_log_mono_time(), 789);
    let event::Which::Sendcan(can) = root.which().unwrap() else {
        panic!("expected sendcan")
    };
    let can = can.unwrap().get(0);
    assert_eq!(can.get_src(), 4);
    assert_eq!(can.get_address(), 123);
    assert_eq!(can.get_dat().unwrap(), [1, 2, 3]);
}
