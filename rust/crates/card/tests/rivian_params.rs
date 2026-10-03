use openpilot_card::brands::rivian::{parameters, ParamsInput};
use openpilot_cereal::car_capnp::car_params::{self, SafetyModel};
use openpilot_params::Params;

#[test]
fn stock_r1_retains_radar_unavailable_and_stock_longitudinal() {
    let root = tempfile::tempdir().unwrap();
    let settings = Params::open(root.path(), "d").unwrap();
    let cp = parameters(ParamsInput {
        candidate: "RIVIAN_R1_GEN1",
        fingerprints: &[],
        firmware: &[],
        alpha_long: false,
        settings: &settings,
    })
    .unwrap();
    let reader = cp.get_root_as_reader::<car_params::Reader>().unwrap();
    let safety = reader.get_safety_configs().unwrap().get(0);
    assert_eq!(safety.get_safety_model().unwrap(), SafetyModel::Rivian);
    assert_eq!(safety.get_safety_param(), 0);
    assert!(!reader.get_openpilot_longitudinal_control());
    assert!(reader.get_radar_unavailable());
    assert!(reader.get_pcm_cruise());
}

#[test]
fn alpha_long_preserves_source_flag_despite_unavailable_toggle() {
    let root = tempfile::tempdir().unwrap();
    let settings = Params::open(root.path(), "d").unwrap();
    let cp = parameters(ParamsInput {
        candidate: "RIVIAN_R1_GEN1",
        fingerprints: &[],
        firmware: &[],
        alpha_long: true,
        settings: &settings,
    })
    .unwrap();
    let reader = cp.get_root_as_reader::<car_params::Reader>().unwrap();
    assert!(!reader.get_alpha_longitudinal_available());
    assert!(reader.get_openpilot_longitudinal_control());
    assert_eq!(
        reader
            .get_safety_configs()
            .unwrap()
            .get(0)
            .get_safety_param(),
        1
    );
    assert_eq!(reader.get_longitudinal_actuator_delay(), 0.35);
    assert_eq!(reader.get_v_ego_stopping(), 0.25);
    assert_eq!(reader.get_stop_accel(), 0.);
}
