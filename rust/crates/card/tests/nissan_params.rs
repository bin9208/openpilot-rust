use openpilot_card::brands::nissan::{parameters, ParamsInput};
use openpilot_cereal::car_capnp::car_params::{self, SafetyModel, SteerControlType};
use openpilot_params::Params;

#[test]
fn altima_uses_the_source_eps_bus_and_angle_control() {
    let root = tempfile::tempdir().unwrap();
    let settings = Params::open(root.path(), "d").unwrap();
    settings.put("NNFF", b"1").unwrap();
    let cp = parameters(ParamsInput {
        candidate: "NISSAN_ALTIMA",
        fingerprints: &[],
        firmware: &[],
        alpha_long: true,
        settings: &settings,
    })
    .unwrap();
    let reader = cp.get_root_as_reader::<car_params::Reader>().unwrap();
    assert_eq!(
        reader.get_steer_control_type().unwrap(),
        SteerControlType::Angle
    );
    let safety = reader.get_safety_configs().unwrap().get(0);
    assert_eq!(safety.get_safety_model().unwrap(), SafetyModel::Nissan);
    assert_eq!(safety.get_safety_param(), 1);
    assert!(reader.get_pcm_cruise() && !reader.get_openpilot_longitudinal_control());
    assert!(matches!(
        reader.get_lateral_tuning().which().unwrap(),
        car_params::lateral_tuning::Which::Pid(_)
    ));
    assert!(settings.get("NNFFModelName").unwrap().is_none());
}

#[test]
fn leaf_retains_manual_stop_resume_and_the_standard_eps_bus() {
    let root = tempfile::tempdir().unwrap();
    let settings = Params::open(root.path(), "d").unwrap();
    let cp = parameters(ParamsInput {
        candidate: "NISSAN_LEAF_IC",
        fingerprints: &[],
        firmware: &[],
        alpha_long: false,
        settings: &settings,
    })
    .unwrap();
    let reader = cp.get_root_as_reader::<car_params::Reader>().unwrap();
    assert!(!reader.get_auto_resume_sng());
    assert!(reader.get_radar_unavailable());
    assert_eq!(
        reader
            .get_safety_configs()
            .unwrap()
            .get(0)
            .get_safety_param(),
        0
    );
    assert_eq!(reader.get_steer_actuator_delay(), 0.1);
    assert_eq!(reader.get_steer_limit_timer(), 1.);
}
