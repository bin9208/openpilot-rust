use openpilot_card::{
    brands::tesla::{parameters, ParamsInput},
    vehicle_params,
};
use openpilot_cereal::car_capnp::car_params;
use openpilot_params::Params;

#[test]
fn vehicle_bus_longitudinal_sets_source_safety_flags() {
    let root = tempfile::tempdir().unwrap();
    let settings = Params::open(root.path(), "d").unwrap();
    let fingerprints = vec![(1, vec![(0x3df, 8)]), (2, vec![(0x293, 8)])];
    let cp = parameters(ParamsInput {
        candidate: "TESLA_MODEL_Y",
        fingerprints: &fingerprints,
        firmware: &[],
        alpha_long: true,
        settings: &settings,
    })
    .unwrap();
    let cp = cp.get_root_as_reader::<car_params::Reader>().unwrap();
    assert_eq!(cp.get_flags(), 24);
    assert_eq!(
        cp.get_safety_configs().unwrap().get(0).get_safety_param(),
        5
    );
}

#[test]
fn model_x_retains_missing_torque_catalog_failure() {
    let root = tempfile::tempdir().unwrap();
    let settings = Params::open(root.path(), "d").unwrap();
    let error = parameters(ParamsInput {
        candidate: "TESLA_MODEL_X",
        fingerprints: &[],
        firmware: &[],
        alpha_long: false,
        settings: &settings,
    })
    .err()
    .unwrap();
    assert!(
        matches!(error,openpilot_card::brands::tesla::Error::Baseline(vehicle_params::Error::MissingTorque(candidate)) if candidate=="TESLA_MODEL_X")
    );
}
