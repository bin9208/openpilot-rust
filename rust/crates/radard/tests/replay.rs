use capnp::{message::Builder, serialize};
use openpilot_cereal::log_capnp::{event, LaneChangeState};
use openpilot_radard::{
    controller::Options,
    replay::{identity, log, Inversion},
    wire,
};
use std::{fs, io::Write};

#[test]
fn full_schema_model_retains_stopping_lane_inputs() {
    let mut message = Builder::new_default();
    let mut model = message.init_root::<event::Builder<'_>>().init_model_v2();
    model.reborrow().init_velocity().init_x(3).set(2, 13.);
    model.reborrow().init_lane_line_probs(4).set(1, 0.9);
    model.reborrow().init_lane_line_stds(4).set(1, 0.2);
    model
        .reborrow()
        .init_meta()
        .set_lane_change_state(LaneChangeState::PreLaneChange);
    let mut lines = model.reborrow().init_lane_lines(4);
    lines.reborrow().get(1).init_x(2).set(1, 10.);
    lines.reborrow().get(1).init_y(2).set(1, -1.8);
    let value = wire::model(model.into_reader()).unwrap();
    assert_eq!(value.velocity, [0., 0., 13.]);
    assert_eq!(value.lane_lines[1].x, [0., 10.]);
    assert_eq!(value.lane_lines[1].y[1], f64::from(-1.8f32));
    assert_eq!(value.lane_stds[1], f64::from(0.2f32));
    assert_eq!(value.lane_change_state, "preLaneChange");
}

#[test]
fn full_schema_stream_preserves_missing_inputs_and_replays_compression() {
    let dir = std::env::temp_dir().join(format!("radar-replay-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let mut data = Vec::new();
    for (index, kind) in [0, 1, 3, 0].into_iter().enumerate() {
        let mut message = Builder::new_default();
        let mut event = message.init_root::<event::Builder<'_>>();
        event.set_log_mono_time(1_000_000_000 + index as u64 * 1_000_000);
        event.set_valid(true);
        match kind {
            0 => {
                let mut model = event.init_model_v2();
                model.set_timestamp_eof(900_000_000);
                model.init_position().init_x(2).set(1, 10.);
            }
            1 => {
                event.init_car_state().set_v_ego(5.);
            }
            2 => {
                event.init_live_pose();
            }
            _ => {
                event.init_live_tracks().set_radar_track_flipped(true);
            }
        }
        serialize::write_message(&mut data, &message).unwrap();
    }
    let path = dir.join("rlog");
    fs::write(&path, &data).unwrap();
    let mut expected = Vec::new();
    log::run(
        &path,
        Options::default(),
        Inversion::Recorded,
        &mut expected,
        None,
    )
    .unwrap();
    let values: Vec<serde_json::Value> = String::from_utf8(expected.clone())
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert!(values[0]["skipped_missing"].as_array().unwrap().len() == 2);
    assert_eq!(values[1]["recorded_flipped"], true);
    assert!(values[1]["output"].is_object());
    let zst = dir.join("rlog.zst");
    fs::write(&zst, zstd::stream::encode_all(data.as_slice(), 1).unwrap()).unwrap();
    let bz2 = dir.join("rlog.bz2");
    let mut encoder = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::fast());
    encoder.write_all(&data).unwrap();
    fs::write(&bz2, encoder.finish().unwrap()).unwrap();
    for path in [zst, bz2] {
        let mut actual = Vec::new();
        log::run(
            &path,
            Options::default(),
            Inversion::Recorded,
            &mut actual,
            None,
        )
        .unwrap();
        assert_eq!(actual, expected);
    }
    let before = identity::file_hash(&path).unwrap();
    fs::write(&path, b"changed").unwrap();
    assert_ne!(identity::file_hash(&path).unwrap(), before);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn provider_bytes_invalidate_cache_even_when_option_paths_are_unchanged() {
    let dir = std::env::temp_dir().join(format!("radar-replay-provider-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let asset = dir.join("libprovider.so");
    fs::write(&asset, b"first numerical provider").unwrap();
    let first = serde_json::json!({"source":"candidate","config_path":"same.json","provider":identity::directory_hash(&dir).unwrap()});
    fs::write(&asset, b"second numerical provider").unwrap();
    let second = serde_json::json!({"source":"candidate","config_path":"same.json","provider":identity::directory_hash(&dir).unwrap()});
    assert_ne!(
        identity::cache_key(&first).unwrap(),
        identity::cache_key(&second).unwrap()
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
#[ignore = "requires an explicitly selected verified native numerical provider"]
fn raw_replay_joins_exact_can_batches_and_reports_missing_metadata() {
    use openpilot_cereal::car_capnp::car_params;
    use openpilot_radard::replay::{dbc, raw::Raw};
    let provider =
        std::env::var_os("RADAR_REPLAY_NUMERICS").expect("RADAR_REPLAY_NUMERICS required");
    let dir = std::env::temp_dir().join(format!("radar-replay-raw-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../opendbc_repo/opendbc/dbc");
    for (name, contents) in dbc::generate(&root).unwrap() {
        fs::write(dir.join(name), contents).unwrap();
    }
    let mut cp = Builder::new_default();
    let mut config = cp.init_root::<car_params::Builder<'_>>();
    config.set_car_fingerprint("TOYOTA_PRIUS");
    config.set_radar_time_step(0.05);
    config.init_safety_configs(1);
    let mut raw = Raw::new(
        &serialize::write_message_to_words(&cp),
        &dir,
        std::path::Path::new(&provider),
        Default::default(),
        true,
    )
    .unwrap();
    let mut can = Builder::new_default();
    let mut event = can.init_root::<event::Builder<'_>>();
    event.set_log_mono_time(1_000_000_010);
    let mut frame = event.init_can(1).get(0);
    frame.set_address(0x22f);
    frame.set_src(1);
    frame.set_dat(&[0, 0, 0, 0, 0, 0x37]);
    let (output, errors) = raw
        .feed(
            &serialize::write_message_to_words(&can),
            1_000_000_010,
            true,
        )
        .unwrap();
    assert!(output.is_empty() && errors.is_empty());
    let mut state = Builder::new_default();
    let mut event = state.init_root::<event::Builder<'_>>();
    event.set_log_mono_time(1_000_000_030);
    let mut car = event.init_car_state();
    car.set_v_ego(10.);
    let mut batch = car.init_radar_input();
    batch.set_first_can_mono_time(1_000_000_010);
    batch.set_last_can_mono_time(1_000_000_010);
    batch.set_can_packet_count(1);
    batch.set_receive_mono_time(1_000_000_020);
    let (output, errors) = raw
        .feed(
            &serialize::write_message_to_words(&state),
            1_000_000_030,
            false,
        )
        .unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    // The configured source period initializes on the first completed radar
    // sweep; that sweep must remain unpublished rather than skipping warmup.
    assert!(output.is_empty());
    let mut event = can.get_root::<event::Builder<'_>>().unwrap();
    event.set_log_mono_time(1_050_000_010);
    let event::Can(frames) = event.which().unwrap() else {
        panic!("CAN fixture union");
    };
    frames.unwrap().get(0).set_dat(&[1, 0, 0, 0, 0, 0x38]);
    let (_, errors) = raw
        .feed(
            &serialize::write_message_to_words(&can),
            1_050_000_010,
            true,
        )
        .unwrap();
    assert!(errors.is_empty());
    let mut event = state.get_root::<event::Builder<'_>>().unwrap();
    event.set_log_mono_time(1_050_000_030);
    let event::CarState(car) = event.which().unwrap() else {
        panic!("car fixture union");
    };
    let mut car = car.unwrap();
    car.set_v_ego(20.);
    let mut batch = car.get_radar_input().unwrap();
    batch.set_first_can_mono_time(1_050_000_010);
    batch.set_last_can_mono_time(1_050_000_010);
    batch.set_receive_mono_time(1_050_000_020);
    let (output, errors) = raw
        .feed(
            &serialize::write_message_to_words(&state),
            1_050_000_030,
            false,
        )
        .unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(output.len(), 1);
    let message = serialize::read_message(output[0].as_slice(), Default::default()).unwrap();
    let event::LiveTracks(tracks) = message
        .get_root::<event::Reader<'_>>()
        .unwrap()
        .which()
        .unwrap()
    else {
        panic!("raw replay publication type");
    };
    assert!(tracks.unwrap().get_radar_track_flipped());
    let mut missing = Builder::new_default();
    missing.init_root::<event::Builder<'_>>().init_car_state();
    let (_, errors) = raw
        .feed(
            &serialize::write_message_to_words(&missing),
            1_000_000_040,
            false,
        )
        .unwrap();
    assert_eq!(errors, ["missingBatchMetadata"]);
    fs::remove_dir_all(dir).unwrap();
}
