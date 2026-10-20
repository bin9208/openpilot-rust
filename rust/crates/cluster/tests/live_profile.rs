use capnp::{message::Builder, serialize};
use openpilot_cereal::log_capnp::event;
use openpilot_cluster::live::Input;
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};

fn isolated(name: &str) -> bool {
    if std::env::var_os("CLUSTER_PROFILE_TEST_CHILD").is_some() {
        return true;
    }
    let directory = tempfile::Builder::new()
        .prefix("msgq_rust-probe-cluster-profile-")
        .tempdir_in("/dev/shm")
        .unwrap();
    let prefix = directory
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .strip_prefix("msgq_")
        .unwrap();
    assert!(std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env("CLUSTER_PROFILE_TEST_CHILD", "1")
        .env("OPENPILOT_PREFIX", prefix)
        .env_remove("CEREAL_FAKE")
        .status()
        .unwrap()
        .success());
    false
}

#[test]
fn profiling_is_opt_in_drained_and_measures_actual_status_work() {
    if !isolated("profiling_is_opt_in_drained_and_measures_actual_status_work") {
        return;
    }
    let mut input = Input::isolated(false, 0, 10.0).unwrap();
    let text = input.status_text(11.0, 2, 3).unwrap();
    assert!(input.profile_samples().is_empty());
    input.set_profile_enabled(true);
    assert_eq!(input.status_text(11.0, 2, 3).unwrap(), text);
    let samples = input.profile_samples();
    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0].0, "source.live.status_text");
    assert!(samples[0].1.is_finite() && samples[0].1 >= 0.0);
    assert!(input.profile_samples().is_empty());
    input.status_text(11.0, 2, 3).unwrap();
    input.clear_profile_samples();
    assert!(input.profile_samples().is_empty());
    input.set_profile_enabled(false);
    input.status_text(11.0, 2, 3).unwrap();
    assert!(input.profile_samples().is_empty());
    assert!(input.close());
    assert!(input.close());
    println!("status={text}; samples={samples:?}; repeated no-media close=true");
}

fn decoder_threads() -> BTreeSet<String> {
    std::fs::read_dir("/proc/self/task")
        .unwrap()
        .filter_map(|item| {
            let path = item.ok()?.path();
            std::fs::read_to_string(path.join("comm"))
                .ok()?
                .starts_with("cluster-navi-")
                .then(|| path.file_name().unwrap().to_string_lossy().into_owned())
        })
        .collect()
}
#[test]
fn actual_cereal_poll_projection_and_owned_media_worker_close() {
    if !isolated("actual_cereal_poll_projection_and_owned_media_worker_close") {
        return;
    }
    let mut publisher =
        openpilot_messaging::runtime::PubMaster::isolated(&["carState", "deviceState"]).unwrap();
    let mut input = Input::isolated(false, 50, 10.0).unwrap();
    input.set_profile_enabled(true);
    let mut car = Builder::new_default();
    car.init_root::<event::Builder<'_>>()
        .init_car_state()
        .set_v_ego(20.0);
    car.get_root::<event::Builder<'_>>()
        .unwrap()
        .set_valid(true);
    let mut device = Builder::new_default();
    device
        .init_root::<event::Builder<'_>>()
        .init_device_state()
        .set_started(true);
    publisher
        .send("deviceState", &serialize::write_message_to_words(&device))
        .unwrap();
    publisher
        .send("carState", &serialize::write_message_to_words(&car))
        .unwrap();
    input.poll().unwrap();
    assert_eq!(
        input.project_at(10.0).unwrap().speed_kph.to_bits(),
        72.0f64.to_bits()
    );
    let stages = input.profile_samples();
    assert_eq!(
        stages.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
        vec![
            "source.live.submaster_update",
            "source.live.current_speed",
            "source.live.apply_updates",
            "source.live.car_frame"
        ]
    );
    assert!(stages.iter().all(|(_, ms)| ms.is_finite() && *ms >= 0.0));
    let before = decoder_threads();
    let capacity = openpilot_messaging::services::lookup("carrotNaviMedia")
        .unwrap()
        .queue_size;
    let mut media = openpilot_msgq::Publisher::with_capacity("carrotNaviMedia", capacity).unwrap();
    input.attach_media(openpilot_cluster::media::MediaInput::isolated().unwrap());
    let mut message = Builder::new_default();
    let mut data = message
        .init_root::<event::Builder<'_>>()
        .init_carrot_navi_media();
    data.set_schema_version(1);
    data.set_session_id("owned");
    data.set_kind("render");
    data.set_name("map_main");
    data.set_present(true);
    data.set_message_type(3);
    data.set_flags(1);
    data.set_width(24);
    data.set_height(16);
    data.set_payload(b"invalid owned H264 decode request");
    media
        .send(&serialize::write_message_to_words(&message))
        .unwrap();
    input.project_at(10.1).unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    while decoder_threads() == before && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    let started = decoder_threads();
    assert!(started.len() > before.len());
    assert!(input.close());
    assert!(input.close());
    // pthread_join may return after clear_child_tid but before /proc removes the
    // exiting task. Bound only that kernel observation; close itself stays strict.
    let deadline = Instant::now() + Duration::from_secs(1);
    let mut remaining = decoder_threads();
    while remaining != before && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
        remaining = decoder_threads();
    }
    assert_eq!(
        remaining, before,
        "decoder task lingered after joined close; before={before:?}, started={started:?}"
    );
    println!("actual native Cereal stages={stages:?}; owned media decoder tasks before={before:?} started={started:?} gone={remaining:?} on repeated close");
}
