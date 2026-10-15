use capnp::{message::Builder, serialize};
use openpilot_cereal::car_capnp::car_params;
use openpilot_messaging::state::Poll;
use openpilot_radard::wire;

#[test]
fn only_positive_hyundai_bosch_modes_add_live_tracks_polling() {
    for brand in ["hyundai", "toyota", "volkswagen"] {
        for unavailable in [false, true] {
            for ext in [0, 1 << 17, 1 << 18, (1 << 17) | (1 << 18)] {
                for mode in [-2, -1, 0, 1, 2, 3] {
                    let mut message = Builder::new_default();
                    let mut cp = message.init_root::<car_params::Builder<'_>>();
                    cp.set_brand(brand);
                    cp.set_ext_flags(ext);
                    cp.set_radar_unavailable(unavailable);
                    let options =
                        wire::subscriptions(&serialize::write_message_to_words(&message), mode)
                            .unwrap();
                    let multiple = brand == "hyundai" && ext & (1 << 17) != 0 && mode > 0;
                    match options.poll {
                        Poll::Many(names) => {
                            assert!(multiple);
                            assert_eq!(names, ["modelV2", "liveTracks"]);
                        }
                        Poll::One(name) => {
                            assert!(!multiple);
                            assert_eq!(name, "modelV2");
                        }
                        Poll::All => panic!("source never polls all topics"),
                    }
                    assert_eq!(options.ignore_alive, ["livePose"]);
                    assert_eq!(options.ignore_valid, ["livePose"]);
                    assert!(options.ignore_frequency.is_empty());
                }
            }
        }
    }
}

#[test]
fn track_wakes_consume_inputs_but_only_model_wakes_publish() {
    use openpilot_cereal::log_capnp::event;
    use openpilot_messaging::state::State;
    use openpilot_radard::{
        controller::Options,
        daemon::{self, Radar},
    };
    let mut config = Builder::new_default();
    let mut cp = config.init_root::<car_params::Builder<'_>>();
    cp.set_brand("hyundai");
    cp.set_ext_flags(1 << 17);
    let subscription = wire::subscriptions(&serialize::write_message_to_words(&config), 1).unwrap();
    let mut state = State::new(&daemon::SERVICES, subscription).unwrap();
    let mut radar = Radar::new(Options::default());
    let mut publications = 0;
    for index in 0..40u64 {
        let mut message = Builder::new_default();
        let mut event = message.init_root::<event::Builder<'_>>();
        let now = 1_000_000_000 + index * 25_000_000;
        event.set_log_mono_time(now);
        if index % 2 == 0 {
            event.init_live_tracks();
        } else {
            event.init_model_v2().set_timestamp_eof(now);
        }
        state
            .update(
                now as f64 * 1e-9,
                &[serialize::write_message_to_words(&message)],
            )
            .unwrap();
        let output = radar.process(&state, now).unwrap();
        assert_eq!(output.is_some(), index % 2 == 1);
        publications += usize::from(output.is_some());
    }
    assert_eq!(publications, 20);
}

#[test]
#[cfg(feature = "native-skip-miri")]
fn actual_ipc_bosch_tracks_wakes_drain_without_extra_publication() {
    use openpilot_cereal::log_capnp::event;
    use openpilot_messaging::runtime::{PubMaster, SubMaster};
    use openpilot_radard::{
        controller::Options,
        daemon::{self, Radar},
    };
    use std::{
        env, fs,
        process::Command,
        time::{Duration, Instant},
    };
    if env::var_os("RADARD_BOSCH_IPC_CHILD").is_none() {
        let prefix = format!("rust-probe-radard-bosch-{}", std::process::id());
        let namespace = std::path::Path::new("/dev/shm").join(format!("msgq_{prefix}"));
        fs::create_dir(&namespace).unwrap();
        let status = Command::new(env::current_exe().unwrap())
            .args([
                "--exact",
                "actual_ipc_bosch_tracks_wakes_drain_without_extra_publication",
                "--nocapture",
            ])
            .env("RADARD_BOSCH_IPC_CHILD", "1")
            .env("OPENPILOT_PREFIX", prefix)
            .env_remove("CEREAL_FAKE")
            .status()
            .unwrap();
        fs::remove_dir_all(namespace).unwrap();
        assert!(status.success());
        return;
    }
    let mut config = Builder::new_default();
    let mut cp = config.init_root::<car_params::Builder<'_>>();
    cp.set_brand("hyundai");
    cp.set_ext_flags(1 << 17);
    let options = wire::subscriptions(&serialize::write_message_to_words(&config), 1).unwrap();
    let mut publisher = PubMaster::isolated(&["modelV2", "liveTracks"]).unwrap();
    let mut subscriber = SubMaster::isolated(&daemon::SERVICES, options).unwrap();
    let mut radar = Radar::new(Options::default());
    for index in 0..20u64 {
        let mut message = Builder::new_default();
        let mut event = message.init_root::<event::Builder<'_>>();
        event.set_log_mono_time(1_000_000_000 + index * 50_000_000);
        event.init_live_tracks();
        publisher
            .send("liveTracks", &serialize::write_message_to_words(&message))
            .unwrap();
        let started = Instant::now();
        subscriber.update(Duration::from_secs(1)).unwrap();
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "radar-only wake waited for model timeout"
        );
        assert!(subscriber.state.topic("liveTracks").unwrap().updated);
        assert!(!subscriber.state.topic("modelV2").unwrap().updated);
        assert!(radar.process(&subscriber.state, 1).unwrap().is_none());
    }
    let mut message = Builder::new_default();
    message.init_root::<event::Builder<'_>>().init_model_v2();
    publisher
        .send("modelV2", &serialize::write_message_to_words(&message))
        .unwrap();
    subscriber.update(Duration::from_secs(1)).unwrap();
    assert!(radar.process(&subscriber.state, 2).unwrap().is_some());
}
