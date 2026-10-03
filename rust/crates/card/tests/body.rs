use openpilot_card::{
    brands::body,
    core::{ApplyInput, Vehicle},
    firmware::Firmware,
};
use openpilot_cereal::car_capnp::{car_control, car_params};
use openpilot_params::Params;

#[test]
fn body_is_a_native_vehicle_with_angle_safety_and_source_zero_torque_deadband() {
    let root = tempfile::tempdir().unwrap();
    let settings = Params::open(root.path(), "d").unwrap();
    let message = body::parameters("COMMA_BODY", &Vec::<Firmware>::new(), &settings).unwrap();
    let cp = message.get_root_as_reader::<car_params::Reader>().unwrap();
    assert!(cp.get_not_car());
    assert_eq!(cp.get_mass(), 9.);
    assert_eq!(
        cp.get_steer_control_type().unwrap(),
        car_params::SteerControlType::Angle
    );
    let bytes = capnp::serialize::write_message_to_words(&message);
    let dbc =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../opendbc_repo/opendbc/dbc");
    let mut vehicle = body::Body::new(&bytes, &dbc, 0).unwrap();
    vehicle.update(&[], 0).unwrap();
    let mut control = capnp::message::Builder::new_default();
    control
        .init_root::<car_control::Builder>()
        .set_enabled(true);
    let result = vehicle
        .apply(ApplyInput {
            control: control.get_root_as_reader().unwrap(),
            now_ns: 0,
            model: None,
            radar: None,
        })
        .unwrap();
    let actuators = result
        .actuators
        .get_root_as_reader::<car_control::actuators::Reader>()
        .unwrap();
    assert_eq!(actuators.get_accel(), -10.);
    assert_eq!(actuators.get_torque(), -10.);
    assert_eq!(result.can.len(), 1);
}
