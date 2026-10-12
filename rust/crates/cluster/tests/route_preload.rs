use openpilot_cereal::log_capnp::event;
use openpilot_cluster::{
    route_replay::preload::{ParseRequest, Session},
    runtime_args::CornerSource,
};

fn car(time: u64, display: f32) -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    let mut root = message.init_root::<event::Builder<'_>>();
    root.set_log_mono_time(time);
    root.set_valid(true);
    let mut car = root.init_car_state();
    car.set_v_ego(10.0);
    car.set_v_ego_cluster(display);
    let mut bytes = Vec::new();
    capnp::serialize::write_message(&mut bytes, &message).unwrap();
    bytes
}
#[test]
fn preload_normalizes_sorts_and_preserves_parser_history_after_empty_and_failed_files() {
    let root = tempfile::tempdir().unwrap();
    let first = root.path().join("first.raw");
    let mut bytes = car(3_000_000_000, 11.0);
    bytes.extend(car(2_000_000_000, 12.0));
    std::fs::write(&first, bytes).unwrap();
    let mut session = Session::new(CornerSource::Live);
    let response = session.parse(ParseRequest {
        generation: 4,
        index: 0,
        path: first.clone(),
    });
    assert_eq!(response.generation, 4);
    assert_eq!(response.path, first);
    let frames = response.result.unwrap();
    assert_eq!(
        frames
            .iter()
            .map(|frame| frame.t.to_bits())
            .collect::<Vec<_>>(),
        vec![0.0_f64.to_bits(), 1.0_f64.to_bits()]
    );
    let empty = root.path().join("empty.raw");
    std::fs::write(&empty, []).unwrap();
    assert!(session
        .parse(ParseRequest {
            generation: 4,
            index: 1,
            path: empty
        })
        .result
        .unwrap()
        .is_empty());
    assert!(session
        .parse(ParseRequest {
            generation: 4,
            index: 2,
            path: root.path().join("missing")
        })
        .result
        .is_err());
    let last = root.path().join("last.raw");
    std::fs::write(&last, car(4_000_000_000, 0.0)).unwrap();
    let response = session.parse(ParseRequest {
        generation: 5,
        index: 3,
        path: last,
    });
    assert_eq!(response.index, 3);
    let frames = response.result.unwrap();
    assert_eq!(frames[0].t.to_bits(), 2.0_f64.to_bits());
    assert_eq!(
        frames[0].state.display_speed_kph.unwrap().to_bits(),
        0.0_f64.to_bits()
    );
}
