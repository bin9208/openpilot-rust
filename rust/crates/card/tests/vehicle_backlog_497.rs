//! Actual state/CAN regressions for the three independently repaired source defects.
use openpilot_can::{dbc::Dbc, packer::Packer, Frame, Packet};
use openpilot_card::brands::{hyundai, tesla, volkswagen};
use openpilot_cereal::car_capnp::{car_control, car_params, car_state};
use openpilot_params::Params;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

fn dbc_root() -> PathBuf {
    std::env::var_os("HYUNDAI_DBC_DIR").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../opendbc_repo/opendbc/dbc"),
        PathBuf::from,
    )
}

fn frame(packer: &mut Packer, name: &str, bus: u8, values: &[(&str, f64)]) -> Frame {
    Frame {
        address: packer.dbc.message(name).unwrap().address,
        data: packer.pack(name, values, None).unwrap(),
        bus,
    }
}

#[test]
fn mqb_all_23_identities_complete_first_and_continuous_updates() {
    let catalog: serde_json::Value =
        serde_json::from_str(include_str!("../data/vehicle.json")).unwrap();
    let platforms: Vec<_> = catalog["platforms"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["brand"] == "volkswagen" && p["flags"].as_u64().unwrap() & 18 == 0)
        .collect();
    assert_eq!(platforms.len(), 23);
    for platform in platforms {
        let candidate = platform["candidate"].as_str().unwrap();
        // Manual, automatic and direct-drive admission use real parameter construction.
        for fingerprint in [vec![], vec![(0xad, 8)], vec![(0x187, 8)]] {
            let root = tempfile::tempdir().unwrap();
            let settings = Params::open(root.path(), "d").unwrap();
            let fingerprints = [(0, fingerprint)];
            let cp = volkswagen::parameters(volkswagen::ParamsInput {
                candidate,
                fingerprints: &fingerprints,
                firmware: &[],
                alpha_long: false,
                settings: &settings,
            })
            .unwrap();
            let wheel_factor = cp
                .get_root_as_reader::<car_params::Reader>()
                .unwrap()
                .get_wheel_speed_factor();
            let mut vehicle = volkswagen::Volkswagen::new(volkswagen::Setup {
                params_bytes: &capnp::serialize::write_message_to_words(&cp),
                dbc_root: &dbc_root(),
                settings,
                fingerprints: &fingerprints,
                now_ns: 2_000_000_000,
            })
            .unwrap();
            let initial = vehicle.update(&[], 2_000_000_000).unwrap();
            let initial = initial.get_root_as_reader::<car_state::Reader>().unwrap();
            assert_eq!(initial.get_v_ego_raw(), 0., "{candidate}");
            assert!(initial.get_standstill(), "{candidate}");
            let mut packer = Packer::new(Arc::clone(&vehicle.state.pt.dbc));
            for tick in 1..=32 {
                let now = 2_000_000_000 + tick * 10_000_000;
                let pressed = tick < 16;
                let frames = vec![
                    frame(
                        &mut packer,
                        "ESP_19",
                        0,
                        &[
                            ("ESP_VL_Radgeschw_02", 36.),
                            ("ESP_VR_Radgeschw_02", 36.),
                            ("ESP_HL_Radgeschw_02", 36.),
                            ("ESP_HR_Radgeschw_02", 36.),
                        ],
                    ),
                    frame(&mut packer, "LH_EPS_03", 0, &[("EPS_Lenkmoment", 81.)]),
                    frame(
                        &mut packer,
                        "GRA_ACC_01",
                        0,
                        &[("GRA_Tip_Setzen", f64::from(u8::from(pressed)))],
                    ),
                ];
                let output = vehicle
                    .update(
                        &[Packet {
                            mono_time: now,
                            frames,
                        }],
                        now,
                    )
                    .unwrap();
                let cs = output.get_root_as_reader::<car_state::Reader>().unwrap();
                assert_eq!(
                    cs.get_v_ego_raw(),
                    10. * wheel_factor,
                    "{candidate} tick {tick}"
                );
                assert!(cs.get_v_ego().is_finite() && cs.get_a_ego().is_finite());
                assert!(!cs.get_standstill());
                assert!(cs.get_steering_pressed());
                if tick == 1 || tick == 16 {
                    let buttons = cs.get_button_events().unwrap();
                    assert_eq!(buttons.len(), 1);
                    assert_eq!(
                        buttons.get(0).get_type().unwrap(),
                        car_state::button_event::Type::SetCruise
                    );
                    assert_eq!(buttons.get(0).get_pressed(), pressed);
                }
            }
            assert_eq!(vehicle.state.extras.frame, 33);
        }
    }
}

#[test]
fn tesla_cancel_press_and_release_survive_simultaneous_infotainment_events() {
    for has_vehicle_bus in [false, true] {
        let flags = if has_vehicle_bus {
            tesla::HAS_VEHICLE_BUS
        } else {
            0
        };
        let mut state =
            tesla::state::State::new(&dbc_root(), flags, 12., true, 2_000_000_000).unwrap();
        let mut party = Packer::new(Arc::clone(&state.party.dbc));
        let mut vehicle = Packer::new(Arc::new(
            Dbc::load(&dbc_root().join("tesla_model3_vehicle.dbc")).unwrap(),
        ));
        // Cancel is held for four samples; infotainment changes on both cancel edges.
        for (tick, (acc, touch)) in [(4., 0.), (0., 3.), (4., 3.), (4., 3.), (4., 3.), (4., 0.)]
            .into_iter()
            .enumerate()
        {
            let now = 2_000_000_000 + tick as u64 * 10_000_000;
            let mut frames = vec![frame(
                &mut party,
                "DAS_control",
                2,
                &[("DAS_accState", acc)],
            )];
            if has_vehicle_bus {
                frames.push(frame(
                    &mut vehicle,
                    "UI_status2",
                    1,
                    &[("UI_activeTouchPoints", touch)],
                ));
            }
            let output = state
                .update(
                    &[Packet {
                        mono_time: now,
                        frames,
                    }],
                    now,
                )
                .unwrap();
            let cs = output.get_root_as_reader::<car_state::Reader>().unwrap();
            let events: Vec<_> = cs
                .get_button_events()
                .unwrap()
                .iter()
                .map(|e| (e.get_type().unwrap(), e.get_pressed()))
                .collect();
            let expected = if tick == 1 || tick == 5 {
                let mut expected = vec![(car_state::button_event::Type::Cancel, tick == 1)];
                if has_vehicle_bus {
                    expected.push((car_state::button_event::Type::Lkas, tick == 1));
                }
                expected
            } else {
                vec![]
            };
            assert_eq!(
                events, expected,
                "vehicle bus {has_vehicle_bus}, tick {tick}"
            );
        }
    }
}

fn hyundai_state(
    offset: u8,
    hda2: bool,
) -> (
    tempfile::TempDir,
    hyundai::state::State,
    hyundai::controller::Controller,
) {
    let root = tempfile::tempdir().unwrap();
    let settings = Params::open(root.path(), "d").unwrap();
    let config = hyundai::config::CarConfig {
        candidate: "KIA_EV6".into(),
        flags: hyundai::flags::CANFD
            | hyundai::flags::EV
            | hyundai::flags::ALT_BUTTONS
            | if hda2 { hyundai::flags::HDA2 } else { 0 },
        ext_flags: 0,
        bsm: false,
        longitudinal: false,
        pcm: true,
        wheel_speed_factor: 1.,
        wheelbase: 2.9,
        steer_ratio: 14.,
        bus: hyundai::bus::CanBus::with_offset(offset, hda2, 0),
    };
    let mut state = hyundai::state::State::new(
        config,
        settings,
        hyundai::state::CanSetup {
            path: &dbc_root().join("hyundai_canfd_generated.dbc"),
            fingerprints: &[],
            now_ns: 2_000_000_000,
        },
    )
    .unwrap();
    state.metric = true;
    let mut out = state.out.get_root::<car_state::Builder>().unwrap();
    out.set_v_ego(20.);
    let mut cruise = out.reborrow().init_cruise_state();
    cruise.set_enabled(true);
    cruise.set_speed(20.);
    let controller = hyundai::controller::Controller::new(&state).unwrap();
    (root, state, controller)
}

fn capture(
    state: &hyundai::state::State,
    counter: f64,
) -> BTreeMap<&'static str, hyundai::wire::Values> {
    let mut values: hyundai::wire::Values = state
        .inputs
        .pt
        .dbc
        .message("CRUISE_BUTTONS_ALT")
        .unwrap()
        .signals
        .iter()
        .map(|s| (s.name.clone(), 0.))
        .collect();
    values.insert("COUNTER".into(), counter);
    [("cruise_buttons_msg", values)].into_iter().collect()
}

#[test]
fn hyundai_scalar_alt_capture_emits_cancel_on_correct_bus_and_wraps_counter() {
    for offset in [0, 4] {
        for hda2 in [false, true] {
            let (_root, state, mut controller) = hyundai_state(offset, hda2);
            let captures = capture(&state, 255.);
            let mut control = capnp::message::Builder::new_default();
            control
                .init_root::<car_control::Builder>()
                .init_cruise_control()
                .set_cancel(true);
            let frames = controller
                .buttons
                .messages(
                    &mut controller.writer,
                    (control.get_root_as_reader().unwrap(), &state),
                    (&controller.settings, 26, &captures),
                )
                .unwrap();
            assert_eq!(frames.len(), 1);
            assert_eq!(
                frames[0].bus,
                if hda2 {
                    state.config.bus.ecan
                } else {
                    state.config.bus.cam
                }
            );
            let mut expected = hyundai::wire::CanWriter::from_dbc(Arc::clone(&state.inputs.pt.dbc));
            let mut values = captures["cruise_buttons_msg"].clone();
            values.insert("COUNTER".into(), 0.);
            values.insert("CRUISE_BUTTONS".into(), 4.);
            assert_eq!(
                frames[0].data,
                expected
                    .frame("CRUISE_BUTTONS_ALT", frames[0].bus, &values, None)
                    .unwrap()
                    .data
            );
        }
    }
}

#[test]
fn hyundai_alt_buttons_preserve_brake_hold_parking_interlocks_and_missing_capture() {
    for guard in 0..4 {
        let (_root, mut state, mut controller) = hyundai_state(0, false);
        let captures = if guard == 3 {
            BTreeMap::new()
        } else {
            capture(&state, 10.)
        };
        let mut out = state.out.get_root::<car_state::Builder>().unwrap();
        out.set_brake_pressed(guard == 0);
        out.set_brake_hold_active(guard == 1);
        out.set_parking_brake(guard == 2);
        let mut control = capnp::message::Builder::new_default();
        control
            .init_root::<car_control::Builder>()
            .init_cruise_control()
            .set_cancel(true);
        let frames = controller
            .buttons
            .messages(
                &mut controller.writer,
                (control.get_root_as_reader().unwrap(), &state),
                (&controller.settings, 26, &captures),
            )
            .unwrap();
        assert!(frames.is_empty());
    }
}

#[test]
fn hyundai_alt_speed_and_resume_reuse_cached_template_and_count_once_per_send() {
    let (_root, mut state, mut controller) = hyundai_state(4, true);
    controller.settings.spam[2] = 3;
    let captures = capture(&state, 254.);
    let mut control = capnp::message::Builder::new_default();
    let mut cc = control.init_root::<car_control::Builder>();
    cc.reborrow().init_hud_control().set_set_speed(25.);
    // Establish unchanged-speed history through the real inactive policy before requesting spam.
    let initial = controller
        .buttons
        .messages(
            &mut controller.writer,
            (control.get_root_as_reader().unwrap(), &state),
            (&controller.settings, 1, &captures),
        )
        .unwrap();
    assert!(initial.is_empty());
    control
        .get_root::<car_control::Builder>()
        .unwrap()
        .set_enabled(true);
    for (tick, expected_counter) in [(40, 255.), (41, 1.), (42, 4.)] {
        // Only the first call has a fresh scalar capture. Later sends use its owned copy.
        let empty = BTreeMap::new();
        let input = if tick == 40 { &captures } else { &empty };
        let frames = controller
            .buttons
            .messages(
                &mut controller.writer,
                (control.get_root_as_reader().unwrap(), &state),
                (&controller.settings, tick, input),
            )
            .unwrap();
        assert_eq!(frames.len(), 3);
        assert!(frames
            .iter()
            .all(|frame| frame.bus == state.config.bus.ecan && frame.data == frames[0].data));
        let mut values = captures["cruise_buttons_msg"].clone();
        values.insert("COUNTER".into(), expected_counter);
        values.insert("CRUISE_BUTTONS".into(), 1.);
        let mut expected = hyundai::wire::CanWriter::from_dbc(Arc::clone(&state.inputs.pt.dbc));
        assert_eq!(
            frames[0].data,
            expected
                .frame("CRUISE_BUTTONS_ALT", state.config.bus.ecan, &values, None)
                .unwrap()
                .data
        );
    }
    // A real captured button press still suppresses speed spam; explicit resume keeps its cadence.
    state.cruise_buttons.push_back(2);
    let frames = controller
        .buttons
        .messages(
            &mut controller.writer,
            (control.get_root_as_reader().unwrap(), &state),
            (&controller.settings, 43, &BTreeMap::new()),
        )
        .unwrap();
    assert!(frames.is_empty());
    control
        .get_root::<car_control::Builder>()
        .unwrap()
        .get_cruise_control()
        .unwrap()
        .set_resume(true);
    let frames = controller
        .buttons
        .messages(
            &mut controller.writer,
            (control.get_root_as_reader().unwrap(), &state),
            (&controller.settings, 44, &BTreeMap::new()),
        )
        .unwrap();
    assert_eq!(frames.len(), 3);
}
