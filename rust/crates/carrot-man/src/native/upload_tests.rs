use super::*;
use std::{
    io,
    net::{Ipv4Addr, TcpListener},
    os::unix::fs::PermissionsExt,
};

#[test]
fn configured_webdav_failure_does_not_use_the_api_or_export_credentials() {
    let root = tempfile::tempdir().unwrap();
    let trap = TcpListener::bind("127.0.0.1:0").unwrap();
    trap.set_nonblocking(true).unwrap();
    let settings = root.path().join("web_settings.json");
    fs::write(
        &settings,
        serde_json::json!({"web_upload_url":format!("http://{}", trap.local_addr().unwrap())})
            .to_string(),
    )
    .unwrap();
    let private = root.path().join("private_upload.json");
    fs::write(&private, "{\"url\":\"http://invalid.test/\",\"username\":\"fixture-user\",\"password\":\"fixture-secret\"}").unwrap();
    fs::set_permissions(&private, fs::Permissions::from_mode(0o600)).unwrap();
    let params = Params::open(&root.path().join("params"), "d").unwrap();
    let config = Config {
        params_root: None,
        memory_root: root.path().join("memory"),
        prefix: "d".into(),
        bind: Ipv4Addr::LOCALHOST,
        udp_port: 0,
        tcp_port: 0,
        http_port: 0,
        route_port: 0,
        kisa_port: 0,
        command_port: 0,
        broadcast_port: 0,
        geos_library: None,
        data_root: root.path().to_owned(),
        repo_root: root.path().to_owned(),
        web_settings: settings,
    };
    let upload = Upload {
        config: &config,
        params: &params,
    };
    assert!(upload.web("fixture", true).is_none());
    assert!(matches!(trap.accept(), Err(error) if error.kind() == io::ErrorKind::WouldBlock));
    snapshot::toggles(&params, root.path()).unwrap();
    let toggles = fs::read_to_string(root.path().join("toggle_values.json")).unwrap();
    assert!(!toggles.contains("fixture-secret"));
    assert!(!toggles.contains("fixture-user"));
}
