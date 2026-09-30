#![cfg(feature = "native")]

use openpilot_cereal::log_capnp::event;
use openpilot_messaging::{
    runtime::{PubMaster, SubMaster},
    state::{Options, Poll},
};
use std::{
    env,
    process::Command,
    time::{Duration, Instant},
};

fn packet(name: &str, timestamp: u64, valid: bool, velocity: f32) -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    let mut root = message.init_root::<event::Builder<'_>>();
    root.set_log_mono_time(timestamp);
    root.set_valid(valid);
    match name {
        "carState" => root.init_car_state().set_v_ego(velocity),
        "deviceState" => {
            root.init_device_state();
        }
        "carrotMan" => {
            root.init_carrot_man();
        }
        _ => panic!("invalid fixture topic"),
    }
    capnp::serialize::write_message_to_words(&message)
}

#[test]
fn source_sized_ipc_preserves_conflation_payloads_and_checks() {
    if env::var_os("RUST_MESSAGE_STATE_CHILD").is_none() {
        let namespace = tempfile::Builder::new()
            .prefix("msgq_rust-probe-state-")
            .tempdir_in("/dev/shm")
            .unwrap();
        let prefix = namespace
            .path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .strip_prefix("msgq_")
            .unwrap();
        let status = Command::new(env::current_exe().unwrap())
            .args([
                "--exact",
                "source_sized_ipc_preserves_conflation_payloads_and_checks",
                "--nocapture",
            ])
            .env("RUST_MESSAGE_STATE_CHILD", "1")
            .env("OPENPILOT_PREFIX", prefix)
            .env_remove("CEREAL_FAKE")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }
    let names = ["carState", "deviceState", "carrotMan"];
    let mut publisher = PubMaster::isolated(&names).unwrap();
    let mut subscriber = SubMaster::isolated(
        &names,
        Options {
            poll: Poll::One("carState".to_owned()),
            ..Options::default()
        },
    )
    .unwrap();
    assert!(subscriber.state.topic("carrotMan").unwrap().valid);
    publisher
        .send("deviceState", &packet("deviceState", 1, false, 0.0))
        .unwrap();
    let started = Instant::now();
    subscriber.update(Duration::from_millis(30)).unwrap();
    assert!(started.elapsed() >= Duration::from_millis(25));
    assert!(subscriber.state.topic("deviceState").unwrap().updated);
    assert!(!subscriber.state.topic("carState").unwrap().seen);
    assert!(!subscriber.state.all_valid(&[]).unwrap());
    for value in 0..20 {
        publisher
            .send(
                "carState",
                &packet("carState", 100 + value, true, value as f32),
            )
            .unwrap();
    }
    publisher
        .send("carrotMan", &packet("carrotMan", 200, true, 0.0))
        .unwrap();
    subscriber.update(Duration::from_millis(100)).unwrap();
    let topic = subscriber.state.topic("carState").unwrap();
    assert_eq!(topic.log_mono_time, 119);
    assert!(topic.updated && topic.valid && topic.alive);
    for _ in 0..1000 {
        let event::Which::CarState(car) = topic.event().unwrap().which().unwrap() else {
            panic!("wrong topic")
        };
        assert_eq!(car.unwrap().get_v_ego(), 19.0);
    }
    assert!(publisher.all_readers_updated("carState").unwrap());
    assert!(subscriber.state.all_alive(&[]).unwrap());
    subscriber.update(Duration::ZERO).unwrap();
    assert!(!subscriber.state.topic("carState").unwrap().updated);
    assert_eq!(
        subscriber.state.topic("carState").unwrap().log_mono_time,
        119
    );
    let frame = subscriber.state.frame();
    publisher
        .send(
            "carState",
            &[0_u32.to_le_bytes(), 100_u32.to_le_bytes()].concat(),
        )
        .unwrap();
    assert!(subscriber.update(Duration::ZERO).is_err());
    assert_eq!(subscriber.state.frame(), frame);
}

#[test]
fn runtime_simulation_keeps_seen_messages_alive_without_overriding_validity() {
    if env::var_os("RUST_MESSAGE_SIMULATION_CHILD").is_none() {
        let namespace = tempfile::Builder::new()
            .prefix("msgq_rust-probe-simulation-")
            .tempdir_in("/dev/shm")
            .unwrap();
        let prefix = namespace
            .path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .strip_prefix("msgq_")
            .unwrap();
        let status = Command::new(env::current_exe().unwrap())
            .args([
                "--exact",
                "runtime_simulation_keeps_seen_messages_alive_without_overriding_validity",
            ])
            .env("RUST_MESSAGE_SIMULATION_CHILD", "1")
            .env("OPENPILOT_PREFIX", prefix)
            .env("SIMULATION", "1")
            .env_remove("CEREAL_FAKE")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }
    let mut publisher = PubMaster::for_runtime(&["carState"]).unwrap();
    let mut subscriber = SubMaster::for_runtime(&["carState"], Options::default()).unwrap();
    subscriber.state.update(100.0, &[]).unwrap();
    assert!(!subscriber.state.topic("carState").unwrap().alive);
    assert!(subscriber.state.topic("carState").unwrap().frequency_ok);
    publisher
        .send("carState", &packet("carState", 1, false, 0.0))
        .unwrap();
    subscriber.update(Duration::ZERO).unwrap();
    let received = subscriber.state.topic("carState").unwrap().receive_time;
    subscriber.state.update(received + 100.0, &[]).unwrap();
    assert!(subscriber.state.topic("carState").unwrap().alive);
    assert!(!subscriber.state.all_valid(&[]).unwrap());
}
