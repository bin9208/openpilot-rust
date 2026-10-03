use openpilot_card::{
    brands::psa::{self, ParamsInput},
    vehicle_params,
};
use openpilot_params::Params;

#[test]
fn source_normal_parameters_fail_before_any_setting_effect() {
    let root = tempfile::tempdir().unwrap();
    let settings = Params::open(root.path(), "d").unwrap();
    let error = psa::parameters(ParamsInput {
        candidate: "PSA_PEUGEOT_208",
        fingerprints: &[],
        firmware: &[],
        alpha_long: false,
        settings: &settings,
    })
    .err()
    .unwrap();
    assert!(
        matches!(error, psa::Error::Baseline(vehicle_params::Error::MissingTorque(candidate)) if candidate == "PSA_PEUGEOT_208")
    );
    assert!(settings.get("NNFFModelName").unwrap().is_none());
    assert!(settings
        .get("LongitudinalPersonalityMax")
        .unwrap()
        .is_none());
}

#[test]
fn candidate_only_constructor_fails_before_setting_effects_when_dbc_absent() {
    let root = tempfile::tempdir().unwrap();
    let settings = Params::open(root.path(), "d").unwrap();
    let mut message = openpilot_card::core::Message::new_default();
    message
        .init_root::<openpilot_cereal::car_capnp::car_params::Builder>()
        .set_car_fingerprint("PSA_PEUGEOT_208");
    let bytes = capnp::serialize::write_message_to_words(&message);
    let error = psa::Psa::new(psa::Setup {
        params_bytes: &bytes,
        dbc_root: root.path(),
        settings: Params::open(root.path(), "d").unwrap(),
        fingerprints: &[],
        now_ns: 0,
    })
    .err()
    .unwrap();
    assert!(matches!(error, psa::Error::DbcLoad {
        source: openpilot_can::Error::Io(ref cause), ..
    } if cause.kind() == std::io::ErrorKind::NotFound));
    assert!(settings.get("NNFFModelName").unwrap().is_none());
    assert!(settings
        .get("LongitudinalPersonalityMax")
        .unwrap()
        .is_none());
}

#[test]
fn direct_state_update_reports_missing_source_method() {
    let state = psa::State::new().unwrap();
    let result = state.update();
    assert!(matches!(
        result,
        Err(psa::Error::SourceMethod("parse_wheel_speeds"))
    ));
}
