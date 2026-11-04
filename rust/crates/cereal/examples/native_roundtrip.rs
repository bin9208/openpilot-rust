//! SDK integration evidence: compare these exact wire bytes with the original
//! full Python cereal schema in CI. This program does not start a daemon.
use openpilot_cereal::log_capnp::event;

fn print(bytes: &[u8]) {
    for byte in bytes {
        print!("{byte:02x}");
    }
    println!();
}

fn main() {
    let mut message = capnp::message::Builder::new_default();
    let mut root = message.init_root::<event::Builder<'_>>();
    root.set_log_mono_time(123456789);
    root.set_valid(true);
    root.init_car_state().set_v_ego(12.5);
    let bytes = capnp::serialize::write_message_to_words(&message);
    let reader = capnp::serialize::read_message_from_flat_slice(
        &mut bytes.as_slice(),
        capnp::message::ReaderOptions::new(),
    )
    .unwrap();
    let root = reader.get_root::<event::Reader<'_>>().unwrap();
    assert!(root.get_valid());
    assert_eq!(root.get_log_mono_time(), 123456789);
    let event::CarState(state) = root.which().unwrap() else {
        panic!("expected carState event");
    };
    assert_eq!(state.unwrap().get_v_ego(), 12.5);
    print(&bytes);

    let mut message = capnp::message::Builder::new_default();
    let mut root = message.init_root::<event::Builder<'_>>();
    root.set_log_mono_time(987654321);
    root.set_valid(false);
    root.set_log_message("SDK path | capnp 원본");
    let bytes = capnp::serialize::write_message_to_words(&message);
    let reader = capnp::serialize::read_message_from_flat_slice(
        &mut bytes.as_slice(),
        capnp::message::ReaderOptions::new(),
    )
    .unwrap();
    let root = reader.get_root::<event::Reader<'_>>().unwrap();
    assert!(!root.get_valid());
    assert_eq!(root.get_log_mono_time(), 987654321);
    let event::LogMessage(text) = root.which().unwrap() else {
        panic!("expected logMessage event");
    };
    assert_eq!(text.unwrap().to_str().unwrap(), "SDK path | capnp 원본");
    print(&bytes);
}
