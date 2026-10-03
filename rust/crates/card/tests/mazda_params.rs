use openpilot_card::brands::mazda::{parameters, ParamsInput};
use openpilot_cereal::car_capnp::car_params;
use openpilot_params::Params;

#[test]
fn cx5_2022_retains_stock_cruise_and_torque_tuning() {
    let root = tempfile::tempdir().unwrap();
    let settings = Params::open(root.path(), "d").unwrap();
    let cp = parameters(ParamsInput {
        candidate: "MAZDA_CX5_2022",
        fingerprints: &[],
        firmware: &[],
        alpha_long: true,
        settings: &settings,
    })
    .unwrap();
    let cp = cp.get_root_as_reader::<car_params::Reader>().unwrap();
    assert!(!cp.get_dashcam_only());
    assert_eq!(cp.get_min_steer_speed(), 0.);
    assert!(cp.get_pcm_cruise() && !cp.get_openpilot_longitudinal_control());
    assert_eq!(cp.get_flags(), 1);
    assert_eq!(
        cp.get_safety_configs()
            .unwrap()
            .get(0)
            .get_safety_model()
            .unwrap(),
        car_params::SafetyModel::Mazda
    );
    assert!(matches!(
        cp.get_lateral_tuning().which().unwrap(),
        car_params::lateral_tuning::Which::Torque(_)
    ));
}

#[test]
fn legacy_models_retain_source_low_speed_gate_and_dashcam_status() {
    let root = tempfile::tempdir().unwrap();
    let settings = Params::open(root.path(), "d").unwrap();
    let cp = parameters(ParamsInput {
        candidate: "MAZDA_3",
        fingerprints: &[],
        firmware: &[],
        alpha_long: false,
        settings: &settings,
    })
    .unwrap();
    let cp = cp.get_root_as_reader::<car_params::Reader>().unwrap();
    assert!(cp.get_dashcam_only());
    assert_eq!(cp.get_min_steer_speed(), 12.5);
    assert_eq!(cp.get_steer_limit_timer(), 0.8);
    assert_eq!(cp.get_steer_actuator_delay(), 0.1);
}
