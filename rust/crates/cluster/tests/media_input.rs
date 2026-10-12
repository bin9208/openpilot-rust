use capnp::{message::Builder, serialize};
use openpilot_cereal::log_capnp::event;

fn isolated() -> bool {
    if std::env::var_os("CLUSTER_MEDIA_TEST_CHILD").is_some() {
        return true;
    }
    let directory = tempfile::Builder::new()
        .prefix("msgq_rust-probe-cluster-media-")
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
        .args([
            "--exact",
            "owned_media_ipc_preserves_sequence_session_and_stale_boundaries",
            "--nocapture"
        ])
        .env("CLUSTER_MEDIA_TEST_CHILD", "1")
        .env("OPENPILOT_PREFIX", prefix)
        .status()
        .unwrap()
        .success());
    false
}
fn image(sequence: u64, session: &str, present: bool) -> Vec<u8> {
    let mut message = Builder::new_default();
    let event = message.init_root::<event::Builder<'_>>();
    let mut data = event.init_carrot_navi_media();
    data.set_schema_version(1);
    data.set_session_id(session);
    data.set_kind("render");
    data.set_name("map_main");
    data.set_sequence(sequence);
    data.set_present(present);
    data.set_message_type(1);
    data.set_format_or_reason(1);
    data.set_width(24);
    data.set_height(16);
    data.set_payload(b"real owned image payload");
    serialize::write_message_to_words(&message)
}
#[test]
fn owned_media_ipc_preserves_sequence_session_and_stale_boundaries() {
    if !isolated() {
        return;
    }
    let capacity = openpilot_messaging::services::lookup("carrotNaviMedia")
        .unwrap()
        .queue_size;
    let mut publisher =
        openpilot_msgq::Publisher::with_capacity("carrotNaviMedia", capacity).unwrap();
    let mut input = openpilot_cluster::media::MediaInput::isolated().unwrap();
    publisher.send(&image(1, "first", true)).unwrap();
    publisher.send(&image(u64::MAX, "first", true)).unwrap();
    let state = input.update_at(None, 10.0).unwrap();
    assert_eq!(state.received_count, 2);
    assert_eq!(state.media[0].sequence, u64::MAX);
    assert_eq!(
        state.media[0].data.as_deref(),
        Some(b"real owned image payload".as_slice())
    );
    assert_eq!(state.items.len(), 28);
    assert!(state.connected);
    assert!(!input.update_at(None, 13.0).unwrap().map_stream_stalled);
    assert!(
        !input
            .update_at(
                Some(&openpilot_cluster::models::NaviLiveState::default()),
                13.001
            )
            .unwrap()
            .map_stream_stalled
    );
    assert!(
        input
            .update_at(
                Some(&openpilot_cluster::models::NaviLiveState::default()),
                13.002
            )
            .unwrap()
            .map_stream_stalled
    );
    publisher.send(&image(2, "second", true)).unwrap();
    let state = input.update_at(None, 14.0).unwrap();
    assert_eq!(state.session_id, "second");
    assert_eq!(state.media.len(), 1);
    publisher.send(&image(3, "second", false)).unwrap();
    let state = input.update_at(None, 14.1).unwrap();
    assert!(!state.media[0].present);
    assert_eq!(state.map_frame_age_ms, None);
    let state = input.update_at(None, 17.101).unwrap();
    assert!(!state.connected);
    assert!(state.media.is_empty());
    println!(
        "owned media IPC observable: {}",
        serde_json::to_string(&state).unwrap()
    );
}
