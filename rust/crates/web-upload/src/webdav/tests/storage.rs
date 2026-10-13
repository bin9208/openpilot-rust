use super::fixture::{file, Fixture, State};
use super::*;

#[test]
fn streaming_route_video_is_atomic_encoded_and_bounded() {
    let fixture = Fixture::new(State::default());
    let root = tempfile::tempdir().unwrap();
    let video = vec![42; 3 * 1024 * 1024 + 7];
    file(root.path(), "qcamera.ts", &video);
    file(root.path(), "rlog.zst", b"fixture route log");
    let names = vec!["qcamera.ts".into(), "rlog.zst".into()];
    let mut maximum = 0;
    let mut progress = |event: Progress<'_>| {
        maximum = maximum.max(event.chunk);
        Ok(())
    };
    assert!(fixture
        .client
        .upload_folder(
            super::super::LocalFiles {
                folder: root.path(),
                names: &names
            },
            super::super::RouteSegment {
                directory: "테스트 Car device",
                name: "route|--0"
            },
            &mut Observer {
                cancel: None,
                progress: Some(&mut progress)
            }
        )
        .unwrap());
    assert!(maximum > 0 && maximum <= STREAM_BUFFER);
    let state = fixture.state();
    assert_eq!(state.files.len(), 2);
    assert_eq!(
        state.files
            ["/openpilot/routes/%ED%85%8C%EC%8A%A4%ED%8A%B8%20Car%20device/route%7C--0/qcamera.ts"],
        video
    );
    assert_eq!(
        state.files
            ["/openpilot/routes/%ED%85%8C%EC%8A%A4%ED%8A%B8%20Car%20device/route%7C--0/rlog.zst"],
        b"fixture route log"
    );
    assert_eq!(
        state
            .requests
            .iter()
            .filter(|(method, _, _)| method == "MOVE")
            .count(),
        2
    );
    assert!(state
        .requests
        .iter()
        .all(|(_, _, auth)| auth == "Basic Zml4dHVyZS11c2VyOmZpeHR1cmUtc2VjcmV0"));
}

#[test]
fn tmux_and_settings_are_separate_from_routes_without_private_result_urls() {
    let fixture = Fixture::new(State::default());
    let root = tempfile::tempdir().unwrap();
    file(root.path(), "tmux.log", b"fixture tmux");
    file(root.path(), "toggles.json", b"{\"Setting\":\"1\"}");
    let metadata = [
        ("git_branch".into(), Value::Text("codex/test".into())),
        ("car_name".into(), Value::Text("Test Car".into())),
        ("dongle_id".into(), Value::Text("fixture-device".into())),
    ]
    .into_iter()
    .collect();
    let response = fixture
        .client
        .upload_tmux(
            &metadata,
            &root.path().join("tmux.log"),
            Some(&root.path().join("toggles.json")),
        )
        .unwrap();
    assert_eq!(response.status, 201);
    let body = response.text().unwrap();
    assert!(!body.contains("http"));
    assert!(!body.contains("fixture-secret"));
    let payload: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(payload["target"], "webdav");
    assert_eq!(payload["paths"].as_array().unwrap().len(), 2);
    assert!(fixture
        .state()
        .files
        .keys()
        .all(|key| key.starts_with("/openpilot/codex__test/Test%20Car%20fixture-device/")));
}
