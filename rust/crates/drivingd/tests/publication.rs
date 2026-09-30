use openpilot_cereal::{
    car_capnp::car_state,
    custom_capnp::carrot_man,
    log_capnp::{event, radar_state},
};
use openpilot_driving_modeld::{
    clock,
    publication::{Output, Publication, Sources},
    state::State,
};
use openpilot_modeld::{model_wire::ModelTiming, parse::Normal, prediction::DrivingPrediction};

fn normal<const N: usize>() -> Normal<N> {
    Normal {
        mean: [0.; N],
        std: [1.; N],
    }
}

#[test]
fn simulation_pose_is_timestamped_after_desire_processing_and_camera_pose_keeps_capture_time() {
    let prediction = DrivingPrediction {
        plan: std::array::from_fn(|i| {
            let mut row = [0.; 15];
            row[0] = i as f32;
            row[3] = 10.;
            row
        }),
        plan_std: [[1.; 15]; 33],
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
        direct_action: None,
    };
    let mut car = capnp::message::Builder::new_default();
    car.init_root::<car_state::Builder>();
    let mut navigation = capnp::message::Builder::new_default();
    navigation.init_root::<carrot_man::Builder>();
    let mut radar = capnp::message::Builder::new_default();
    radar.init_root::<radar_state::Builder>();
    for simulation in [false, true] {
        let mut state = State::new(0., 0., 0.);
        let mut publication = Publication::default();
        let mut after_desire = 0;
        let output = publication
            .build(
                &mut state,
                Output {
                    prediction: &prediction,
                    timing: ModelTiming {
                        log_mono_time: clock::timestamp().unwrap(),
                        frame_id: 1,
                        frame_id_extra: 1,
                        camera_state_frame_id: 1,
                        frame_drop: 0.,
                        timestamp_eof: 42,
                        model_execution_time: 0.,
                        valid: true,
                    },
                    simulation,
                    dropped: 0,
                    raw_predictions: None,
                },
                Sources {
                    car: car.get_root_as_reader().unwrap(),
                    navigation: navigation.get_root_as_reader().unwrap(),
                    radar: radar.get_root_as_reader().unwrap(),
                    lateral_active: false,
                    live_lateral_delay: 0.,
                },
                Default::default,
                |_| {
                    after_desire = clock::timestamp().unwrap();
                    None
                },
            )
            .unwrap();
        let message = capnp::serialize::read_message(
            std::io::Cursor::new(output.pose),
            capnp::message::ReaderOptions::new(),
        )
        .unwrap();
        let event = message.get_root::<event::Reader>().unwrap();
        assert!(event.get_valid());
        let event::CameraOdometry(pose) = event.which().unwrap() else {
            panic!("expected pose");
        };
        let stamp = pose.unwrap().get_timestamp_eof();
        if simulation {
            assert!(after_desire > 0);
            assert!(stamp >= after_desire && stamp <= clock::timestamp().unwrap());
        } else {
            assert_eq!(stamp, 42);
        }
    }
}
