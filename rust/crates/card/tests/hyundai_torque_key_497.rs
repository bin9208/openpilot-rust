//! Exact orphan-key repair, not a newly measured or vehicle-accepted calibration.
use openpilot_card::{
    brands::hyundai::{self, flags, Hyundai, ParamsInput},
    firmware::{Ecu, Firmware},
};
use openpilot_cereal::car_capnp::car_params::{self, SafetyModel, SteerControlType};
use openpilot_params::Params;

#[test]
fn exact_hybrid_key_constructs_full_parameters_across_safety_and_nnff_modes() {
    let artifacts = std::env::var_os("HYUNDAI_TORQUE_KEY_EVIDENCE").map(std::path::PathBuf::from);
    if let Some(path) = &artifacts {
        std::fs::create_dir_all(path).unwrap();
    }
    for offset in [0, 4] {
        for hda2 in [false, true] {
            for angle in [false, true] {
                for nnff in [false, true] {
                    for alpha_long in [false, true] {
                        for with_firmware in [false, true] {
                            let root = tempfile::tempdir().unwrap();
                            let settings = Params::open(root.path(), "d").unwrap();
                            settings.put_bool("NNFF", nnff).unwrap();
                            settings
                                .put("CanfdHDA2", if hda2 { b"1" } else { b"0" })
                                .unwrap();
                            let ecan = offset + u8::from(hda2);
                            let mut camera = vec![(0x2a4, 32)];
                            if angle {
                                camera.push((0xcb, 24));
                            }
                            let fingerprints = vec![
                                (ecan, vec![(0x105, 32), (0x1cf, 8), (0x1ba, 8), (0x130, 8)]),
                                (offset + 2, camera),
                            ];
                            let firmware = if with_firmware {
                                vec![Firmware {
                                    ecu: Ecu::Eps,
                                    fw_version: b"owned DL3 HEV EPS 2024".to_vec(),
                                    ..Default::default()
                                }]
                            } else {
                                vec![]
                            };
                            let cp = Hyundai::parameters(ParamsInput {
                                candidate: "KIA_K5_DL3_24_HEV", fingerprints: &fingerprints,
                                firmware: &firmware, alpha_long, is_release: false, settings: &settings,
                            }).expect("exact existing hybrid tune must resolve through the public parameter API");
                            let reader = cp.get_root_as_reader::<car_params::Reader>().unwrap();
                            assert_eq!(
                                reader.get_car_fingerprint().unwrap().to_str().unwrap(),
                                "KIA_K5_DL3_24_HEV"
                            );
                            assert_eq!(reader.get_max_lateral_accel(), 2.5);
                            assert_eq!(
                                reader.get_steer_control_type().unwrap(),
                                if angle {
                                    SteerControlType::Angle
                                } else {
                                    SteerControlType::Torque
                                }
                            );
                            if !angle {
                                let car_params::lateral_tuning::Which::Torque(tune) =
                                    reader.get_lateral_tuning().which().unwrap()
                                else {
                                    panic!("torque union")
                                };
                                let tune = tune.unwrap();
                                assert_eq!(tune.get_lat_accel_factor(), 2.5);
                                assert_eq!(tune.get_friction(), 0.1);
                                assert_eq!(tune.get_lat_accel_offset(), 0.);
                            }
                            for value in [
                                reader.get_mass(),
                                reader.get_wheelbase(),
                                reader.get_steer_ratio(),
                                reader.get_rotational_inertia(),
                                reader.get_tire_stiffness_front(),
                                reader.get_tire_stiffness_rear(),
                            ] {
                                assert!(value.is_finite() && value > 0.);
                            }
                            assert_eq!(reader.get_openpilot_longitudinal_control(), alpha_long);
                            assert_eq!(reader.get_pcm_cruise(), !alpha_long);
                            assert_eq!(reader.get_flags() & flags::HYBRID, flags::HYBRID);
                            let safety = reader.get_safety_configs().unwrap();
                            assert_eq!(safety.len(), if offset == 4 { 2 } else { 1 });
                            if offset == 4 {
                                assert_eq!(
                                    safety.get(0).get_safety_model().unwrap(),
                                    SafetyModel::NoOutput
                                );
                            }
                            let active = safety.get(safety.len() - 1);
                            assert_eq!(
                                active.get_safety_model().unwrap(),
                                SafetyModel::HyundaiCanfd
                            );
                            assert_eq!(
                                active.get_safety_param(),
                                flags::safety::HYBRID_GAS
                                    | if alpha_long { flags::safety::LONG } else { 0 }
                                    | if hda2 { flags::safety::LKA_STEERING } else { 0 }
                            );
                            let config = hyundai::config::CarConfig::decode(
                                &capnp::serialize::write_message_to_words(&cp),
                                &settings,
                            )
                            .unwrap();
                            assert_eq!(config.bus.ecan, ecan);
                            assert_eq!(config.bus.cam, offset + 2);
                            assert!(settings.get("NNFFModelName").unwrap().is_none());
                            if let Some(path) = &artifacts {
                                let name = format!(
                                    "o{offset}-h{}-a{}-n{}-l{}-f{}",
                                    u8::from(hda2),
                                    u8::from(angle),
                                    u8::from(nnff),
                                    u8::from(alpha_long),
                                    u8::from(with_firmware)
                                );
                                std::fs::write(
                                    path.join(format!("{name}.bin")),
                                    capnp::serialize::write_message_to_words(&cp),
                                )
                                .unwrap();
                            }
                        }
                    }
                }
            }
        }
    }
}
