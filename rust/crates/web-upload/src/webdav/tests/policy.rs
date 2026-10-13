use super::fixture::{file, Fixture, State};
use super::*;
use std::net::TcpListener;

#[test]
fn auth_and_permission_statuses_never_report_success() {
    for (method, status) in [
        ("MKCOL", 401),
        ("MKCOL", 403),
        ("PUT", 401),
        ("PUT", 403),
        ("MOVE", 403),
    ] {
        let fixture = Fixture::new(State {
            fail_method: Some((method, status)),
            ..State::default()
        });
        let root = tempfile::tempdir().unwrap();
        file(root.path(), "rlog.zst", b"route log");
        let error = fixture
            .client
            .upload_folder(
                super::super::LocalFiles {
                    folder: root.path(),
                    names: &["rlog.zst".into()],
                },
                super::super::RouteSegment {
                    directory: "car device",
                    name: "segment",
                },
                &mut Observer::default(),
            )
            .unwrap_err();
        assert!(error.to_string().contains(&format!("HTTP {status}")));
        assert!(fixture.state().files.is_empty());
        assert_eq!(fixture.state().puts, usize::from(method != "MKCOL"));
    }
}

#[test]
fn traversal_rejection_occurs_before_network_or_file_access() {
    let fixture = Fixture::new(State::default());
    let root = tempfile::tempdir().unwrap();
    for (directory, segment, filename) in [
        ("..", "segment", "rlog.zst"),
        ("car device", "../secret", "rlog.zst"),
        ("car device", "segment", "../secret"),
        ("car device", "segment", "a\\secret"),
        ("car device", "segment", "bad\nheader"),
    ] {
        assert!(fixture
            .client
            .upload_folder(
                super::super::LocalFiles {
                    folder: root.path(),
                    names: &[filename.into()]
                },
                super::super::RouteSegment {
                    directory,
                    name: segment
                },
                &mut Observer::default()
            )
            .is_err());
    }
    assert!(fixture.state().requests.is_empty());
}

#[test]
fn redirect_is_refused_and_credentials_never_reach_another_origin() {
    let foreign = TcpListener::bind("127.0.0.1:0").unwrap();
    foreign.set_nonblocking(true).unwrap();
    let fixture = Fixture::new(State {
        fail_method: Some(("MKCOL", 307)),
        redirect: format!("http://{}/foreign/", foreign.local_addr().unwrap()),
        ..State::default()
    });
    let root = tempfile::tempdir().unwrap();
    let error = fixture
        .client
        .upload_folder(
            super::super::LocalFiles {
                folder: root.path(),
                names: &[],
            },
            super::super::RouteSegment {
                directory: "car device",
                name: "segment",
            },
            &mut Observer::default(),
        )
        .unwrap_err();
    assert!(error.to_string().contains("HTTP 307"));
    assert!(matches!(foreign.accept(), Err(error) if error.kind() == io::ErrorKind::WouldBlock));
    assert_eq!(fixture.state().requests.len(), 1);
}

#[test]
fn health_uses_webdav_options_and_does_not_create_api_sessions() {
    let fixture = Fixture::new(State::default());
    assert_eq!(fixture.client.health()["ok"], true);
    assert_eq!(fixture.state().requests.len(), 1);
    assert_eq!(fixture.state().requests[0].0, "OPTIONS");
    assert_eq!(fixture.state().requests[0].1, "/openpilot/");
}
