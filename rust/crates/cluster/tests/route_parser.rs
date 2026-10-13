use openpilot_cereal::log_capnp::event;
use openpilot_cluster::{route_replay::Parser, runtime_args::CornerSource};

fn car(time: u64, speed: f32, display: f32) -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    let mut root = message.init_root::<event::Builder<'_>>();
    root.set_log_mono_time(time);
    root.set_valid(true);
    let mut car = root.init_car_state();
    car.set_v_ego(speed);
    car.set_v_ego_cluster(display);
    let mut bytes = Vec::new();
    capnp::serialize::write_message(&mut bytes, &message).unwrap();
    bytes
}

#[test]
fn parser_keeps_ordered_cereal_frames_and_display_speed_history_across_files() {
    let mut parser = Parser::new(CornerSource::Live);
    let mut bytes = car(1_000_000_000, 10.0, 11.0);
    bytes.extend(car(1_010_000_000, 20.0, 21.0));
    let frames = parser.parse_bytes(&bytes).unwrap();
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].t.to_bits(), 1.0_f64.to_bits());
    assert_eq!(frames[0].state.speed_kph.to_bits(), 36.0_f64.to_bits());
    assert_eq!(frames[1].state.speed_kph.to_bits(), 72.0_f64.to_bits());
    let next = parser.parse_bytes(&car(2_000_000_000, 15.0, 0.0)).unwrap();
    assert_eq!(
        next[0].state.display_speed_kph.unwrap().to_bits(),
        0.0_f64.to_bits()
    );
    assert!(parser.parse_bytes(&[0, 1, 2]).is_err());
}

#[test]
fn parser_explicit_front_only_overrides_corner_support_without_process_environment() {
    let mut message = capnp::message::Builder::new_default();
    let mut root = message.init_root::<event::Builder<'_>>();
    root.set_log_mono_time(900_000_000);
    root.init_car_params().set_ext_flags(1 << 12);
    let mut bytes = Vec::new();
    capnp::serialize::write_message(&mut bytes, &message).unwrap();
    bytes.extend(car(1_000_000_000, 10.0, 10.0));
    let mut native = Parser::new(CornerSource::Live);
    native.set_front_radar_only(false);
    let enabled = native.parse_bytes(&bytes).unwrap();
    let mut front = Parser::new(CornerSource::Live);
    front.set_front_radar_only(true);
    let disabled = front.parse_bytes(&bytes).unwrap();
    assert!(enabled[0].state.corner_radar_supported);
    assert!(!disabled[0].state.corner_radar_supported);
}
