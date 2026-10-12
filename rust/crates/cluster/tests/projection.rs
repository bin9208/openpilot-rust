use capnp::{message::Builder, serialize};
use openpilot_cereal::log_capnp::event;
use openpilot_cluster::live::Projection;

#[test]
fn real_cereal_car_message_projects_source_display_fields() {
    let mut message = Builder::new_default();
    {
        let mut event = message.init_root::<event::Builder<'_>>();
        event.set_log_mono_time(10_000_000_000);
        event.set_valid(true);
        let mut car = event.init_car_state();
        car.set_v_ego(20.0);
        car.set_a_ego(1.5);
        car.set_v_ego_cluster(21.0);
        car.set_steering_angle_deg(9.0);
        car.set_left_blinker(true);
        car.set_v_cruise_cluster(100.5);
        car.reborrow().init_cruise_state().set_available(true);
    }
    let bytes = serialize::write_message_to_words(&message);
    let reader =
        serialize::read_message(&mut &bytes[..], capnp::message::ReaderOptions::new()).unwrap();
    let root = reader.get_root::<event::Reader<'_>>().unwrap();
    let mut projection = Projection::default();
    projection.apply(root).unwrap();
    let event::CarState(car) = root.which().unwrap() else {
        panic!("fixture must contain carState");
    };
    let state = projection.car_frame(car.unwrap().into(), 10.0);
    assert_eq!(state.speed_kph.to_bits(), 72.0_f64.to_bits());
    assert_eq!(state.display_speed_kph, Some(75.600_000_000_000_01));
    assert_eq!(state.accel_mps2.to_bits(), 1.5_f64.to_bits());
    assert_eq!(state.steering.to_bits(), 0.2_f64.to_bits());
    assert_eq!(state.cruise_kph, Some(100));
    assert!(state.left_signal);
    assert_eq!(state.lanes.len(), 2);
}
