use openpilot_can::{packer::Packer, parser::Parser, Frame, Packet};
use openpilot_card::{
    brands::psa::{self, ParamsInput},
    core::ApplyInput,
};
use openpilot_cereal::car_capnp::{car_control, car_params, car_state};
use openpilot_params::Params;
use std::{path::PathBuf, sync::Arc};

fn dbc_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../opendbc_repo/opendbc/dbc")
}
fn params(settings: &Params, alpha_long: bool) -> openpilot_card::core::Message {
    psa::parameters(ParamsInput {
        candidate: "PSA_PEUGEOT_208",
        fingerprints: &[],
        firmware: &[],
        alpha_long,
        settings,
    })
    .unwrap()
}
#[test]
fn official_placeholder_selects_finite_angle_dashcam_and_preserves_no_longitudinal() {
    for nnff in [false, true] {
        for alpha in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let settings = Params::open(root.path(), "d").unwrap();
            settings.put_bool("NNFF", nnff).unwrap();
            let message = params(&settings, alpha);
            let cp = message.get_root_as_reader::<car_params::Reader>().unwrap();
            assert!(
                cp.get_dashcam_only() && cp.get_radar_unavailable() && cp.get_steer_at_standstill()
            );
            assert!(
                !cp.get_openpilot_longitudinal_control() && !cp.get_alpha_longitudinal_available()
            );
            assert_eq!(
                cp.get_steer_control_type().unwrap(),
                car_params::SteerControlType::Angle
            );
            assert_eq!(cp.get_max_lateral_accel(), 2.0);
            assert!(matches!(
                cp.get_lateral_tuning().which().unwrap(),
                car_params::lateral_tuning::Which::Pid(_)
            ));
            let safety = cp.get_safety_configs().unwrap().get(0);
            assert_eq!(
                safety.get_safety_model().unwrap(),
                car_params::SafetyModel::Psa
            );
            assert_eq!(safety.get_safety_param(), 0);
            assert!(settings.get("NNFFModelName").unwrap().is_none());
            assert!(settings
                .get("LongitudinalPersonalityMax")
                .unwrap()
                .is_none());
        }
    }
}
#[test]
fn missing_dbc_still_rejects_constructor_before_settings_effects() {
    let root = tempfile::tempdir().unwrap();
    let settings = Params::open(root.path(), "d").unwrap();
    let message = params(&settings, false);
    let error = psa::Psa::new(psa::Setup {
        params_bytes: &capnp::serialize::write_message_to_words(&message),
        dbc_root: root.path(),
        settings: Params::open(root.path(), "d").unwrap(),
        fingerprints: &[],
        now_ns: 0,
    })
    .err()
    .unwrap();
    assert!(
        matches!(error,psa::Error::DbcLoad {source:openpilot_can::Error::Io(ref cause),..} if cause.kind()==std::io::ErrorKind::NotFound)
    );
    assert!(settings
        .get("LongitudinalPersonalityMax")
        .unwrap()
        .is_none());
}
fn frame(packer: &mut Packer, name: &str, bus: u8, values: &[(&str, f64)]) -> Frame {
    Frame {
        address: packer.dbc.message(name).unwrap().address,
        data: packer.pack(name, values, None).unwrap(),
        bus,
    }
}
#[test]
fn actual_three_bus_state_and_controller_preserve_first_update_status_cadence_and_counter_wrap() {
    let root = tempfile::tempdir().unwrap();
    let settings = Params::open(root.path(), "d").unwrap();
    let message = params(&settings, true);
    let mut vehicle = psa::Psa::new(psa::Setup {
        params_bytes: &capnp::serialize::write_message_to_words(&message),
        dbc_root: &dbc_root(),
        settings,
        fingerprints: &[],
        now_ns: 2_000_000_000,
    })
    .unwrap();
    let first = vehicle.update(&[], 2_000_000_000).unwrap();
    assert_eq!(
        first
            .get_root_as_reader::<car_state::Reader>()
            .unwrap()
            .get_v_ego_raw(),
        0.
    );
    let mut packer = Packer::new(Arc::clone(&vehicle.state.main.dbc));
    let mut decoded = Parser::new(Arc::clone(&vehicle.state.main.dbc), 0, 2_000_000_000);
    decoded
        .add("LANE_KEEP_ASSIST", Some(20.), false, 2_000_000_000)
        .unwrap();
    let mut control = openpilot_card::core::Message::new_default();
    let mut cc = control.init_root::<car_control::Builder>();
    cc.set_lat_active(true);
    cc.set_long_active(true);
    cc.init_actuators().set_steering_angle_deg(500.);
    let mut sends = 0;
    for tick in 0..86_u64 {
        let now = 2_000_000_000 + tick * 10_000_000;
        let frames = vec![
            frame(
                &mut packer,
                "Dyn4_FRE",
                0,
                &[
                    ("P263_VehV_VPsvValWhlFrtL", 36.),
                    ("P264_VehV_VPsvValWhlFrtR", 36.),
                    ("P265_VehV_VPsvValWhlBckL", 36.),
                    ("P266_VehV_VPsvValWhlBckR", 36.),
                ],
            ),
            frame(
                &mut packer,
                "STEERING",
                0,
                &[(
                    "DRIVER_TORQUE",
                    if (11..50).contains(&tick) { 6. } else { 0. },
                )],
            ),
            frame(
                &mut packer,
                "IS_DAT_DIRA",
                0,
                &[(
                    "EPS_STATE_LKA",
                    if (60..70).contains(&tick) { 3. } else { 0. },
                )],
            ),
            frame(
                &mut packer,
                "HS2_DYN_UCF_MDD_32D",
                1,
                &[("VEHICLE_STANDSTILL", if tick % 2 == 0 { 1. } else { 0. })],
            ),
            frame(
                &mut packer,
                "Dat_BSI",
                0,
                &[("P013_MainBrake", 1.), ("P103_Com_bRevGear", 1.)],
            ),
            frame(
                &mut packer,
                "Dat_BSI",
                2,
                &[("P013_MainBrake", if tick >= 12 { 1. } else { 0. })],
            ),
        ];
        let state = vehicle
            .update(
                &[Packet {
                    mono_time: now,
                    frames,
                }],
                now,
            )
            .unwrap();
        let cs = state.get_root_as_reader::<car_state::Reader>().unwrap();
        assert_eq!(cs.get_v_ego_raw(), 10.);
        assert!(cs.get_v_ego().is_finite());
        assert_eq!(cs.get_standstill(), tick % 2 == 0);
        assert_eq!(cs.get_brake_pressed(), tick >= 12);
        assert_eq!(
            cs.get_gear_shifter().unwrap(),
            car_state::GearShifter::Drive
        );
        assert_eq!(cs.get_steering_pressed(), (16..50).contains(&tick));
        let output = vehicle
            .apply(ApplyInput {
                control: control.get_root_as_reader().unwrap(),
                now_ns: now,
                model: None,
                radar: None,
            })
            .unwrap();
        assert_eq!(output.can.len(), usize::from(tick % 5 == 0));
        if tick % 5 == 0 {
            assert!(output
                .can
                .iter()
                .all(|frame| frame.bus == 0 && frame.address == 1010));
            decoded
                .update(&[Packet {
                    mono_time: now,
                    frames: output.can,
                }])
                .unwrap();
            assert_eq!(
                decoded.signal("LANE_KEEP_ASSIST", "COUNTER").unwrap(),
                f64::from(sends % 16)
            );
            assert_eq!(
                decoded.signal("LANE_KEEP_ASSIST", "TORQUE_FACTOR").unwrap(),
                100.
            );
            if tick <= 10 {
                assert_eq!(
                    decoded.signal("LANE_KEEP_ASSIST", "STATUS").unwrap(),
                    [3., 4., 2.][sends as usize]
                );
            }
            sends += 1;
        }
    }
    assert_eq!(sends, 18);
    assert_eq!(vehicle.controller.frame, 86);
}
#[test]
fn active_apply_before_state_update_retains_the_actual_missing_eps_boundary() {
    let root = tempfile::tempdir().unwrap();
    let settings = Params::open(root.path(), "d").unwrap();
    let message = params(&settings, false);
    let mut vehicle = psa::Psa::new(psa::Setup {
        params_bytes: &capnp::serialize::write_message_to_words(&message),
        dbc_root: &dbc_root(),
        settings,
        fingerprints: &[],
        now_ns: 0,
    })
    .unwrap();
    let mut control = openpilot_card::core::Message::new_default();
    control
        .init_root::<car_control::Builder>()
        .set_lat_active(true);
    assert!(matches!(
        vehicle.apply(ApplyInput {
            control: control.get_root_as_reader().unwrap(),
            now_ns: 0,
            model: None,
            radar: None
        }),
        Err(psa::Error::Stock("eps_active"))
    ));
}

#[test]
fn timeout_diagnostics_belong_to_the_update_that_detects_them() {
    let root = tempfile::tempdir().unwrap();
    let settings = Params::open(root.path(), "d").unwrap();
    let message = params(&settings, false);
    let mut vehicle = psa::Psa::new(psa::Setup {
        params_bytes: &capnp::serialize::write_message_to_words(&message),
        dbc_root: &dbc_root(),
        settings,
        fingerprints: &[],
        now_ns: 2_000_000_000,
    })
    .unwrap();
    vehicle.update(&[], 2_000_000_000).unwrap();
    assert!(vehicle.take_logs().is_empty());
    let state = vehicle
        .update(
            &[Packet {
                mono_time: 4_000_000_001,
                frames: vec![],
            }],
            4_000_000_001,
        )
        .unwrap();
    // The source parser reports timeout diagnostics immediately, while its
    // CAN_INVALID_CNT=5 debounce retains canValid for the first four failures.
    assert!(state
        .get_root_as_reader::<car_state::Reader>()
        .unwrap()
        .get_can_valid());
    assert!(state
        .get_root_as_reader::<car_state::Reader>()
        .unwrap()
        .get_can_timeout());
    let logs = vehicle.take_logs();
    assert!(logs.iter().any(|log| log
        .message
        .contains("Dyn4_FRE not valid (timeout or missing)")));
    assert!(vehicle.take_logs().is_empty());
    for missed in 2..=5 {
        let now = 4_000_000_000 + missed;
        let state = vehicle
            .update(
                &[Packet {
                    mono_time: now,
                    frames: vec![],
                }],
                now,
            )
            .unwrap();
        assert_eq!(
            state
                .get_root_as_reader::<car_state::Reader>()
                .unwrap()
                .get_can_valid(),
            missed < 5
        );
    }
}
