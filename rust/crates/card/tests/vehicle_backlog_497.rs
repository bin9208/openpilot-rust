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
        for active in [3., 4.] {
            for cancel in [0., 1., 2., 12., 13., 14., 15.] {
                let mut state = tesla_button_state(has_vehicle_bus);
                let mut party = Packer::new(Arc::clone(&state.party.dbc));
                let mut vehicle = Packer::new(Arc::new(
                    Dbc::load(&dbc_root().join("tesla_model3_vehicle.dbc")).unwrap(),
                ));
                // Both original parsers are lazy: an ordinary first update registers DAS_control.
                // The next CAN update, not constructor-time input, establishes the active state.
                let first = state.update(&[], 2_000_000_000).unwrap();
                assert!(tesla_button_events(&first).is_empty());
                assert_eq!(state.extras.das_acc_state_last, Some(0.));
                let mut history = Vec::new();
                // Source pulse is four samples; infotainment changes on both cancel edges.
                for (tick, (acc, touch)) in [
                    (active, 0.),
                    (cancel, 3.),
                    (active, 3.),
                    (active, 3.),
                    (active, 3.),
                    (active, 0.),
                ]
                .into_iter()
                .enumerate()
                {
                    let now = 2_010_000_000 + tick as u64 * 10_000_000;
                    let das = frame(&mut party, "DAS_control", 2, &[("DAS_accState", acc)]);
                    let address = das.address;
                    let bytes = das.data.clone();
                    let mut frames = vec![das];
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
                    history.push(now);
                    assert_eq!(state.autopilot.raw.get(&address), Some(&bytes));
                    assert_eq!(
                        state.autopilot.states[&address]
                            .timestamps
                            .iter()
                            .copied()
                            .collect::<Vec<_>>(),
                        history
                    );
                    assert_eq!(state.extras.das_acc_state_last, Some(acc));
                    let expected = if tick == 1 || tick == 5 {
                        let mut expected = vec![(car_state::button_event::Type::Cancel, tick == 1)];
                        if has_vehicle_bus {
                            expected.push((car_state::button_event::Type::Lkas, tick == 1));
                        }
                        expected
                    } else {
                        vec![]
                    };
                    assert_eq!(tesla_button_events(&output), expected,
                        "vehicle bus {has_vehicle_bus}, active {active}, cancel {cancel}, tick {tick}");
                }
            }
        }
    }
}

fn tesla_button_state(has_vehicle_bus: bool) -> tesla::state::State {
    tesla::state::State::new(
        &dbc_root(),
        if has_vehicle_bus {
            tesla::HAS_VEHICLE_BUS
        } else {
            0
        },
        12.,
        true,
        2_000_000_000,
    )
    .unwrap()
}

fn tesla_button_events(
    message: &openpilot_card::core::Message,
) -> Vec<(car_state::button_event::Type, bool)> {
    message
        .get_root_as_reader::<car_state::Reader>()
        .unwrap()
        .get_button_events()
        .unwrap()
        .iter()
        .map(|event| (event.get_type().unwrap(), event.get_pressed()))
        .collect()
}

#[test]
fn tesla_cold_first_packet_cannot_establish_a_stock_cancel_transition() {
    for has_vehicle_bus in [false, true] {
        let mut state = tesla_button_state(has_vehicle_bus);
        let mut party = Packer::new(Arc::clone(&state.party.dbc));
        // Literal source contract: the first unregistered 4 is lost, so the following 0
        // is not a 4->0 edge. A later actually parsed 4->0 must still emit Cancel.
        for (tick, (acc, expected_last, pressed)) in [
            (4., 0., false),
            (0., 0., false),
            (4., 4., false),
            (0., 0., true),
        ]
        .into_iter()
        .enumerate()
        {
            let now = 2_000_000_000 + tick as u64 * 10_000_000;
            let das = frame(&mut party, "DAS_control", 2, &[("DAS_accState", acc)]);
            let address = das.address;
            let bytes = das.data.clone();
            let output = state
                .update(
                    &[Packet {
                        mono_time: now,
                        frames: vec![das],
                    }],
                    now,
                )
                .unwrap();
            assert_eq!(state.extras.das_acc_state_last, Some(expected_last));
            if tick == 0 {
                assert!(!state.autopilot.raw.contains_key(&address));
                assert!(state.autopilot.states[&address].timestamps.is_empty());
            } else {
                assert_eq!(state.autopilot.raw.get(&address), Some(&bytes));
                assert_eq!(state.autopilot.states[&address].timestamps.len(), tick);
            }
            assert_eq!(
                tesla_button_events(&output),
                if pressed {
                    vec![(car_state::button_event::Type::Cancel, true)]
                } else {
                    vec![]
                }
            );
        }
    }
}

#[test]
fn tesla_wrong_bus_and_partial_packets_do_not_invent_cancel_or_infotainment_edges() {
    for has_vehicle_bus in [false, true] {
        let mut state = tesla_button_state(has_vehicle_bus);
        let mut party = Packer::new(Arc::clone(&state.party.dbc));
        let mut vehicle = Packer::new(Arc::new(
            Dbc::load(&dbc_root().join("tesla_model3_vehicle.dbc")).unwrap(),
        ));
        state.update(&[], 2_000_000_000).unwrap();
        let mut history = Vec::new();
        let mut accepted_bytes = None;
        // Wrong buses (0/2 instead of AP2/vehicle1), then empty input retain old values.
        // Independent touch edges occur during the four-update cancel pulse.
        for tick in 0..8_u64 {
            let now = 2_010_000_000 + tick * 10_000_000;
            let mut frames = Vec::new();
            if tick != 2 && tick != 5 {
                let acc = if tick == 1 || tick == 3 { 0. } else { 4. };
                let touch = if tick == 1 || tick == 4 { 3. } else { 0. };
                let das = frame(
                    &mut party,
                    "DAS_control",
                    if tick == 1 { 0 } else { 2 },
                    &[("DAS_accState", acc)],
                );
                if tick != 1 {
                    history.push(now);
                    accepted_bytes = Some(das.data.clone());
                }
                frames.push(das);
                frames.push(frame(
                    &mut vehicle,
                    "UI_status2",
                    if tick == 1 { 2 } else { 1 },
                    &[("UI_activeTouchPoints", touch)],
                ));
            }
            let packets = if tick == 5 {
                vec![]
            } else {
                vec![Packet {
                    mono_time: now,
                    frames,
                }]
            };
            let output = state.update(&packets, now).unwrap();
            let address = party.dbc.message("DAS_control").unwrap().address;
            assert_eq!(state.autopilot.raw.get(&address), accepted_bytes.as_ref());
            assert_eq!(
                state.autopilot.states[&address]
                    .timestamps
                    .iter()
                    .copied()
                    .collect::<Vec<_>>(),
                history
            );
            let expected = match tick {
                3 => vec![(car_state::button_event::Type::Cancel, true)],
                4 if has_vehicle_bus => vec![(car_state::button_event::Type::Lkas, true)],
                6 if has_vehicle_bus => vec![(car_state::button_event::Type::Lkas, false)],
                7 => vec![(car_state::button_event::Type::Cancel, false)],
                _ => vec![],
            };
            assert_eq!(
                tesla_button_events(&output),
                expected,
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
