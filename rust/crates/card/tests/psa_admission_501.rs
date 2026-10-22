//! The same test compiles against b37057897's existing public registry/core API.
//! Its RED is missing parameter data / RuntimeUnported, never a new-API compile failure.
use openpilot_card::{
    core::{ApplyInput, Message, Vehicle},
    identification::{FingerprintSource, Identification},
    registry,
};
use openpilot_cereal::car_capnp::{car_control, car_params, car_state};
use openpilot_params::Params;
use std::path::PathBuf;

fn identity() -> Identification {
    Identification {
        candidate: "PSA_PEUGEOT_208".into(),
        observed: vec![],
        vin: "00000000000000000".into(),
        firmware: vec![],
        source: FingerprintSource::Fixed,
        exact_match: true,
        cached: false,
        vin_rx_address: None,
        vin_rx_bus: None,
        ecu_responses: vec![],
        fw_query_time: 0.,
        packets: 202,
    }
}
#[test]
fn public_registry_admits_exact_psa_dashcam_parameters() {
    let root = tempfile::tempdir().unwrap();
    let settings = Params::open(root.path(), "d").unwrap();
    let message = registry::parameters(&identity(), &settings, true).unwrap();
    let cp = message.get_root_as_reader::<car_params::Reader>().unwrap();
    assert!(cp.get_dashcam_only() && cp.get_radar_unavailable());
    assert!(!cp.get_openpilot_longitudinal_control());
    assert_eq!(
        cp.get_steer_control_type().unwrap(),
        car_params::SteerControlType::Angle
    );
    assert_eq!(cp.get_max_lateral_accel(), 2.0);
    assert_eq!(
        cp.get_safety_configs()
            .unwrap()
            .get(0)
            .get_safety_model()
            .unwrap(),
        car_params::SafetyModel::Psa
    );
}
#[test]
fn public_registry_constructs_first_update_and_apply_from_known_angle_params() {
    let root = tempfile::tempdir().unwrap();
    let mut message = Message::new_default();
    let mut cp = message.init_root::<car_params::Builder>();
    cp.set_car_fingerprint("PSA_PEUGEOT_208");
    cp.set_brand("psa");
    cp.set_steer_control_type(car_params::SteerControlType::Angle);
    cp.set_dashcam_only(true);
    cp.set_radar_unavailable(true);
    cp.set_pcm_cruise(true);
    cp.set_wheel_speed_factor(1.);
    let bytes = capnp::serialize::write_message_to_words(&message);
    let dbc = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../opendbc_repo/opendbc/dbc");
    let identification = identity();
    let result = registry::Interface::new(registry::Setup {
        identification: &identification,
        params_bytes: &bytes,
        dbc_root: &dbc,
        settings: Params::open(root.path(), "d").unwrap(),
        now_ns: 2_000_000_000,
    });
    let mut interface = match result {
        Ok(interface) => interface,
        Err(error) => {
            assert!(
                format!("{error:?}").contains("RuntimeUnported"),
                "unexpected failure: {error:?}"
            );
            panic!("behavioral RED: existing PSA RuntimeUnported after loading the real DBC: {error:?}");
        }
    };
    assert!(interface.has_controller());
    assert!(interface.parser_readiness().is_none());
    assert_eq!(
        interface.constructor_diagnostics().unwrap(),
        ["DBC: psa_aee2010_r3"]
    );
    assert!(interface.constructor_diagnostics().unwrap().is_empty());
    let first = interface.update(&[], 2_000_000_000).unwrap();
    let state = first.get_root_as_reader::<car_state::Reader>().unwrap();
    assert_eq!(state.get_v_ego_raw(), 0.);
    assert!(state.get_v_ego().is_finite());
    interface.commit_state(state).unwrap();
    interface.set_soft_hold(1);
    assert!(interface.take_param_writes().is_empty());
    let mut control = Message::new_default();
    control
        .init_root::<car_control::Builder>()
        .set_lat_active(true);
    let output = interface
        .apply(ApplyInput {
            control: control.get_root_as_reader().unwrap(),
            now_ns: 2_000_000_000,
            model: None,
            radar: None,
        })
        .unwrap();
    assert_eq!(output.can.len(), 1);
    assert_eq!(output.can[0].address, 1010);
    assert_eq!(output.can[0].bus, 0);
}
