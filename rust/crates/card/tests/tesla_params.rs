use openpilot_card::{
    brands::tesla::{self, parameters, ParamsInput},
    firmware::{Ecu, Firmware},
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
fn model_x_exact_override_preserves_angle_dashcam_safety_and_no_radar_contract() {
    for nnff in [false, true] {
        for alpha_long in [false, true] {
            for vehicle in [false, true] {
                for das in [false, true] {
                    for radar_frame in [false, true] {
                        let root = tempfile::tempdir().unwrap();
                        let settings = Params::open(root.path(), "d").unwrap();
                        settings.put_bool("NNFF", nnff).unwrap();
                        let mut bus1 = Vec::new();
                        if vehicle {
                            bus1.push((0x3df, 8));
                        }
                        if radar_frame {
                            bus1.push((0x410, 8));
                        }
                        let fingerprints =
                            [(1, bus1), (2, if das { vec![(0x293, 8)] } else { vec![] })];
                        // A Model 3-only FSD version must not enable Model X's FSD flag/safety bit.
                        let firmware = [Firmware {
                            ecu: Ecu::Eps,
                            fw_version: b"TeMYG4_Main_0.0.0 (77),E4HP015.04.5".to_vec(),
                            ..Firmware::default()
                        }];
                        let message = parameters(ParamsInput {
                            candidate: "TESLA_MODEL_X",
                            fingerprints: &fingerprints,
                            firmware: &firmware,
                            alpha_long,
                            settings: &settings,
                        })
                        .unwrap();
                        let cp = message.get_root_as_reader::<car_params::Reader>().unwrap();
                        assert!(cp.get_dashcam_only());
                        assert!(cp.get_radar_unavailable());
                        assert_eq!(
                            cp.get_steer_control_type().unwrap(),
                            car_params::SteerControlType::Angle
                        );
                        assert!(matches!(
                            cp.get_lateral_tuning().which().unwrap(),
                            car_params::lateral_tuning::Which::Pid(_)
                        ));
                        assert_eq!(cp.get_max_lateral_accel(), 2.5);
                        assert_eq!(cp.get_openpilot_longitudinal_control(), alpha_long);
                        let expected_flags = if das { 0 } else { tesla::MISSING_DAS_SETTINGS }
                            | if vehicle { tesla::HAS_VEHICLE_BUS } else { 0 }
                            | if alpha_long && vehicle {
                                tesla::AUTO_SPEED_LIMIT
                            } else {
                                0
                            };
                        assert_eq!(cp.get_flags(), expected_flags);
                        let safety = cp.get_safety_configs().unwrap().get(0);
                        assert_eq!(
                            safety.get_safety_model().unwrap(),
                            car_params::SafetyModel::Tesla
                        );
                        assert_eq!(
                            safety.get_safety_param(),
                            u16::from(alpha_long) | if alpha_long && vehicle { 4 } else { 0 }
                        );
                        for value in [
                            cp.get_mass(),
                            cp.get_wheelbase(),
                            cp.get_center_to_front(),
                            cp.get_steer_ratio(),
                            cp.get_rotational_inertia(),
                            cp.get_tire_stiffness_front(),
                            cp.get_tire_stiffness_rear(),
                            cp.get_steer_actuator_delay(),
                            cp.get_max_lateral_accel(),
                        ] {
                            assert!(value.is_finite());
                        }
                        assert!(settings.get("NNFFModelName").unwrap().is_none());
                    }
                }
            }
        }
    }
}

#[test]
fn model_x_admission_still_requires_the_existing_party_dbc() {
    let root = tempfile::tempdir().unwrap();
    let settings = Params::open(root.path(), "d").unwrap();
    let message = parameters(ParamsInput {
        candidate: "TESLA_MODEL_X",
        fingerprints: &[],
        firmware: &[],
        alpha_long: false,
        settings: &settings,
    })
    .unwrap();
    let failure = tesla::Tesla::new(tesla::Setup {
        params_bytes: &capnp::serialize::write_message_to_words(&message),
        dbc_root: root.path(),
        settings,
        fingerprints: &[],
        now_ns: 2_000_000_000,
    })
    .err()
    .unwrap();
    assert!(
        matches!(failure, tesla::Error::Can(openpilot_can::Error::Io(cause))
        if cause.kind() == std::io::ErrorKind::NotFound)
    );
}
