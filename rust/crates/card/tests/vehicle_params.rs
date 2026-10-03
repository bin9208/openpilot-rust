use openpilot_card::vehicle_params::{self, TorqueOptions};
use openpilot_cereal::car_capnp::car_params;

#[test]
fn baseline_reads_platform_and_torque_catalog_without_defaults_for_missing_entries() {
    let params = vehicle_params::baseline("HYUNDAI_IONIQ_5").unwrap();
    let cp = params.get_root_as_reader::<car_params::Reader>().unwrap();
    assert_eq!(
        cp.get_car_fingerprint().unwrap().to_str().unwrap(),
        "HYUNDAI_IONIQ_5"
    );
    assert_eq!(cp.get_wheelbase(), 2.97);
    assert_eq!(cp.get_mass(), 1948.);
    assert!(vehicle_params::baseline("KIA_K5_DL3_24_HEV").is_err());
    assert!(vehicle_params::baseline("unknown").is_err());
}

#[test]
fn torque_union_has_source_gain_and_deadzone() {
    let mut params = vehicle_params::baseline("HYUNDAI_IONIQ_5").unwrap();
    let cp = params.get_root::<car_params::Builder>().unwrap();
    vehicle_params::configure_torque(
        "HYUNDAI_IONIQ_5",
        cp.get_lateral_tuning(),
        TorqueOptions {
            deadzone_deg: 0.2,
            use_steering_angle: false,
        },
    )
    .unwrap();
    let cp = params.get_root_as_reader::<car_params::Reader>().unwrap();
    let car_params::lateral_tuning::Which::Torque(torque) =
        cp.get_lateral_tuning().which().unwrap()
    else {
        panic!("expected torque tuning");
    };
    let torque = torque.unwrap();
    assert_eq!(torque.get_kp(), 1.);
    assert_eq!(torque.get_ki(), 0.1);
    assert!(!torque.get_use_steering_angle());
    assert_eq!(torque.get_steering_angle_deadzone_deg(), 0.2);
}
