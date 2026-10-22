//! These tests use the pre-fix public API so they can also run against b55552dbb.
use openpilot_cereal::{
    car_capnp::car_state,
    custom_capnp::carrot_man,
    log_capnp::{event, jetlink_frame_status, radar_state},
};
use openpilot_driving_modeld::{
    jetlink,
    publication::{Messages, Output, Publication, Sources},
    state::State,
};
use openpilot_jetlink::{
    runtime::Status,
    transition::{Decision, Phase, Source},
};
use openpilot_modeld::{model_wire::ModelTiming, parse::Normal, prediction::DrivingPrediction};

fn normal<const N: usize>() -> Normal<N> {
    Normal {
        mean: [0.; N],
        std: [1.; N],
    }
}

fn prediction(y_std: f32) -> DrivingPrediction {
    let mut prediction = DrivingPrediction {
        plan: [[0.; 15]; 33],
        plan_std: [[9.; 15]; 33],
        pose: normal(),
        wide_euler: normal(),
        road_transform: normal(),
        lanes: normal(),
        edges: normal(),
        leads: normal(),
        lane_prob: [1.; 8],
        lead_prob: [0.; 3],
        meta: [0.; 55],
        desire_state: [0.; 8],
        desire_prediction: [0.; 32],
        direct_action: Some([2., 3.]),
    };
    for row in &mut prediction.plan {
        row[3] = 10.;
    }
    prediction.plan_std[10][1] = y_std;
    prediction
}

fn build(
    publication: &mut Publication,
    state: &mut State,
    prediction: &DrivingPrediction,
    frame: u32,
    raw: Option<&[u8]>,
) -> Messages {
    let mut car = capnp::message::Builder::new_default();
    car.init_root::<car_state::Builder>().set_v_ego(10.);
    let mut navigation = capnp::message::Builder::new_default();
    navigation.init_root::<carrot_man::Builder>();
    let mut radar = capnp::message::Builder::new_default();
    radar.init_root::<radar_state::Builder>();
    publication
        .build(
            state,
            Output {
                prediction,
                timing: ModelTiming {
                    log_mono_time: u64::from(frame) * 50_000_000,
                    frame_id: frame,
                    frame_id_extra: frame,
                    camera_state_frame_id: frame,
                    frame_drop: 0.,
                    timestamp_eof: u64::from(frame) * 50_000_000 - 1,
                    model_execution_time: 0.001,
                    valid: true,
                },
                simulation: false,
                dropped: 0,
                raw_predictions: raw,
            },
            Sources {
                car: car.get_root_as_reader().unwrap(),
                navigation: navigation.get_root_as_reader().unwrap(),
                radar: radar.get_root_as_reader().unwrap(),
                lateral_active: false,
                live_lateral_delay: 0.125,
            },
            Default::default,
            |_| None,
        )
        .unwrap()
}

#[test]
fn issue_17_selected_uncertainty_controls_curvature_and_next_delay() {
    // Distractor columns/samples contain 9 m; only sample 10 position-y is relevant.
    for custom_delay in [0., 0.25] {
        let mut state = State::new(0., 0.05, 0.);
        state.settings.lateral_smooth = 0.1;
        state.settings.custom_lateral_delay = custom_delay;
        let mut publication = Publication::default();
        let mut previous = 0.;
        for (index, (y_std, tau)) in [(0.125, 0.1), (0.1875, 0.175), (0.25, 0.3), (1., 0.3)]
            .into_iter()
            .enumerate()
        {
            let messages = build(
                &mut publication,
                &mut state,
                &prediction(y_std),
                index as u32 + 1,
                None,
            );
            let reader = capnp::serialize::read_message(
                std::io::Cursor::new(messages.model),
                capnp::message::ReaderOptions::new(),
            )
            .unwrap();
            let event::ModelV2(model) =
                reader.get_root::<event::Reader>().unwrap().which().unwrap()
            else {
                panic!("modelV2")
            };
            let curvature = model.unwrap().get_action().unwrap().get_desired_curvature();
            let alpha = 1. - (-0.05_f64 / tau).exp();
            let expected = (alpha * f64::from(0.02_f32) + (1. - alpha) * previous) as f32;
            assert_eq!(curvature, expected, "y_std={y_std}, tau={tau}");
            assert_eq!(state.previous_action.desired_curvature, f64::from(expected));
            let delay = if custom_delay > 0. {
                custom_delay
            } else {
                0.125
            };
            assert!((state.lateral_delay - (delay + tau)).abs() < 1e-12);
            assert!((state.action_times().0 - (delay + tau + 0.075)).abs() < 1e-12);
            previous = f64::from(expected);
        }
    }
}

#[test]
fn issue_17_nan_uncertainty_does_not_poison_the_following_finite_frame() {
    let mut state = State::new(0., 0.05, 0.);
    state.settings.lateral_smooth = 0.1;
    let mut reference_state = State::new(0., 0.05, 0.);
    reference_state.settings.lateral_smooth = 0.1;
    let mut publication = Publication::default();
    let mut reference_publication = Publication::default();
    for (frame, uncertainty, reference_uncertainty, tau) in
        [(1, f32::NAN, 0., 0.1), (2, 0.25, 0.25, 0.3)]
    {
        assert!(state.action_times().0.is_finite());
        let mut selected = prediction(uncertainty);
        // Exercise plan-derived curvature, which also consumes the previous frame's action time.
        selected.direct_action = None;
        for row in &mut selected.plan {
            row[11] = 0.125;
            row[14] = 0.0625;
        }
        let messages = build(&mut publication, &mut state, &selected, frame, None);
        selected.plan_std[10][1] = reference_uncertainty;
        let reference = build(
            &mut reference_publication,
            &mut reference_state,
            &selected,
            frame,
            None,
        );
        assert!(state.lateral_delay.is_finite());
        assert!(state.action_times().0.is_finite());
        assert!((state.lateral_delay - (0.125 + tau)).abs() < 1e-12);
        assert_eq!(state.action_times(), reference_state.action_times());
        for (actual, expected) in [
            (
                state.previous_action.desired_curvature,
                reference_state.previous_action.desired_curvature,
            ),
            (
                state.previous_action.desired_acceleration,
                reference_state.previous_action.desired_acceleration,
            ),
            (
                state.previous_action.desired_velocity,
                reference_state.previous_action.desired_velocity,
            ),
        ] {
            assert!(actual.is_finite());
            assert_eq!(actual, expected);
        }
        assert_eq!(messages.driving, reference.driving);
        assert_eq!(messages.pose, reference.pose);
        let reader = capnp::serialize::read_message(
            std::io::Cursor::new(&messages.model),
            capnp::message::ReaderOptions::new(),
        )
        .unwrap();
        let root = reader.get_root::<event::Reader>().unwrap();
        assert!(root.get_valid());
        let event::ModelV2(model) = root.which().unwrap() else {
            panic!("modelV2")
        };
        let model = model.unwrap();
        assert!(model
            .get_action()
            .unwrap()
            .get_desired_curvature()
            .is_finite());
        let published_std = model.get_position().unwrap().get_y_std().unwrap().get(10);
        if uncertainty.is_nan() {
            assert!(
                published_std.is_nan(),
                "retain the actual model uncertainty on the wire"
            );
        } else {
            assert_eq!(published_std, uncertainty);
            assert_eq!(
                messages.model, reference.model,
                "the finite frame must fully recover"
            );
        }
    }
}

#[test]
fn issue_49_raw_request_keeps_external_publications_and_native_raw_identity() {
    let native = [13, 42];
    let mut publication = Publication::default();
    let mut state = State::new(0., 0.05, 0.);
    for (index, (source, requested, fresh)) in [
        (Source::Native, true, true),
        (Source::Jetlink, true, true),
        (Source::Jetlink, true, true),
        (Source::Jetlink, false, true),
        (Source::Jetlink, true, false),
        (Source::Native, true, true),
        (Source::Native, false, true),
    ]
    .into_iter()
    .enumerate()
    {
        let frame = index as u32 + 1;
        let raw = jetlink::raw_predictions(requested, source, &native)
            .expect("unsupported external raw diagnostics must not abort a valid frame");
        let mut messages = build(&mut publication, &mut state, &prediction(0.), frame, raw);
        let status = Status {
            decision: Decision {
                source,
                phase: Phase::Active,
                loss_latched: false,
                reset_required: false,
            },
            generation: "ab".repeat(16),
            frame,
            execution_ms: 1.,
            validated: true,
        };
        jetlink::publication(&mut messages, &status, fresh).unwrap();
        for bytes in [&messages.model, &messages.driving, &messages.pose] {
            let reader = capnp::serialize::read_message(
                std::io::Cursor::new(bytes),
                capnp::message::ReaderOptions::new(),
            )
            .unwrap();
            let root = reader.get_root::<event::Reader>().unwrap();
            assert_eq!(root.get_valid(), fresh);
            match root.which().unwrap() {
                event::ModelV2(model) => {
                    let model = model.unwrap();
                    assert_eq!(model.get_frame_id(), frame);
                    assert_eq!(model.get_timestamp_eof(), u64::from(frame) * 50_000_000 - 1);
                    assert_eq!(model.get_velocity().unwrap().get_x().unwrap().get(0), 10.);
                    assert_eq!(
                        model.get_raw_predictions().unwrap(),
                        if requested && source == Source::Native {
                            &native[..]
                        } else {
                            &[]
                        }
                    );
                    assert_eq!(
                        model.get_jetlink().unwrap().get_source().unwrap(),
                        if source == Source::Native {
                            jetlink_frame_status::Source::Native
                        } else {
                            jetlink_frame_status::Source::Jetlink
                        }
                    );
                    assert_eq!(model.get_jetlink().unwrap().get_frame_id(), frame);
                }
                event::DrivingModelData(model) => {
                    let model = model.unwrap();
                    assert_eq!(model.get_frame_id(), frame);
                    assert_eq!(model.get_jetlink().unwrap().get_frame_id(), frame);
                }
                event::CameraOdometry(pose) => assert_eq!(pose.unwrap().get_frame_id(), frame),
                _ => panic!("unexpected publication"),
            }
        }
    }
}

#[test]
fn issue_49_unsupported_raw_request_preserves_complete_external_packets() {
    let publish = |requested| {
        let raw = jetlink::raw_predictions(requested, Source::Jetlink, &[13, 42]).unwrap();
        build(
            &mut Publication::default(),
            &mut State::new(0., 0.05, 0.),
            &prediction(0.),
            42,
            raw,
        )
    };
    let normal = publish(false);
    let requested = publish(true);
    assert_eq!(requested.model, normal.model);
    assert_eq!(requested.driving, normal.driving);
    assert_eq!(requested.pose, normal.pose);
}
