use openpilot_card::{
    brands::chrysler::{parameters, ParamsInput},
    firmware::{Ecu, Firmware},
};
use openpilot_cereal::car_capnp::car_params;
use openpilot_params::Params;

#[test]
fn older_pacifica_detects_the_new_eps_minimum_speed() {
    let root = tempfile::tempdir().unwrap();
    let settings = Params::open(root.path(), "d").unwrap();
    let cp = parameters(ParamsInput {
        candidate: "CHRYSLER_PACIFICA_2018",
        fingerprints: &[(0, vec![(720, 8)])],
        firmware: &[Firmware {
            ecu: Ecu::Eps,
            fw_version: b"6841-owned".to_vec(),
            ..Firmware::default()
        }],
        alpha_long: true,
        settings: &settings,
    })
    .unwrap();
    let reader = cp.get_root_as_reader::<car_params::Reader>().unwrap();
    assert_eq!(reader.get_flags() & 1, 1);
    assert_eq!(reader.get_min_steer_speed(), 17.5);
    assert!(reader.get_enable_bsm());
    assert!(matches!(
        reader.get_lateral_tuning().which().unwrap(),
        car_params::lateral_tuning::Which::Pid(_)
    ));
}

#[test]
fn ram_dt_old_firmware_retains_steering_to_zero() {
    let root = tempfile::tempdir().unwrap();
    let settings = Params::open(root.path(), "d").unwrap();
    let cp = parameters(ParamsInput {
        candidate: "RAM_1500_5TH_GEN",
        fingerprints: &[],
        firmware: &[Firmware {
            ecu: Ecu::Eps,
            fw_version: b"6831".to_vec(),
            ..Firmware::default()
        }],
        alpha_long: false,
        settings: &settings,
    })
    .unwrap();
    let reader = cp.get_root_as_reader::<car_params::Reader>().unwrap();
    assert_eq!(reader.get_min_steer_speed(), 0.);
    assert_eq!(
        reader
            .get_safety_configs()
            .unwrap()
            .get(0)
            .get_safety_param(),
        1
    );
    assert!(!reader.get_dashcam_only());
}

#[test]
fn ram_hd_retains_source_torque_deadzone_and_dashcam_status() {
    let root = tempfile::tempdir().unwrap();
    let settings = Params::open(root.path(), "d").unwrap();
    let cp = parameters(ParamsInput {
        candidate: "RAM_HD_5TH_GEN",
        fingerprints: &[],
        firmware: &[],
        alpha_long: false,
        settings: &settings,
    })
    .unwrap();
    let reader = cp.get_root_as_reader::<car_params::Reader>().unwrap();
    let tune = match reader.get_lateral_tuning().which().unwrap() {
        car_params::lateral_tuning::Which::Torque(tune) => tune.unwrap(),
        _ => panic!("source Ram HD torque tuning"),
    };
    assert!(reader.get_dashcam_only());
    assert_eq!(
        reader
            .get_safety_configs()
            .unwrap()
            .get(0)
            .get_safety_param(),
        2
    );
    assert_eq!(tune.get_steering_angle_deadzone_deg(), 1.);
    assert!(!tune.get_use_steering_angle());
}
