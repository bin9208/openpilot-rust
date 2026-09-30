use openpilot_params::Params;
use openpilot_registration::{Clock, Error, Hardware, Registration, Spinner};
use std::{path::Path, time::Duration};
struct Unused;
impl Hardware for Unused {
    fn serial(&mut self) -> Result<String, Error> {
        panic!("hardware must not run")
    }
    fn imei(&mut self, _: usize) -> Result<openpilot_logmessaged::JsonValue, Error> {
        panic!("hardware must not run")
    }
}
impl Clock for Unused {
    fn monotonic(&mut self) -> f64 {
        panic!("clock must not run")
    }
    fn unix_seconds(&mut self) -> Result<i64, Error> {
        panic!("clock must not run")
    }
    fn sleep(&mut self, _: Duration) -> Result<(), Error> {
        panic!("clock must not run")
    }
}
impl Spinner for Unused {
    fn start(&mut self) -> Result<(), Error> {
        panic!("spinner must not run")
    }
    fn update(&mut self, _: &str) -> Result<(), Error> {
        panic!("spinner must not run")
    }
    fn close(&mut self) -> Result<(), Error> {
        panic!("spinner must not run")
    }
}
#[test]
fn missing_public_key_replaces_existing_identity_without_hardware_or_network() {
    let dir = tempfile::tempdir().unwrap();
    let params = Params::open(&dir.path().join("params"), "d").unwrap();
    params.put("DongleId", b"existing").unwrap();
    let mut logger =
        openpilot_logging::producer::Factory::new(format!("ipc://{}/unused", dir.path().display()))
            .unwrap()
            .logger();
    let result = Registration {
        params: &params,
        persist: dir.path(),
        api_host: "http://127.0.0.1:1",
        source_root: Path::new("/unused"),
        logger: &mut logger,
    }
    .register(&mut Unused, &mut Unused, Some(&mut Unused))
    .unwrap();
    assert!(result.text_eq("UnregisteredDevice"));
    assert_eq!(
        params.get("DongleId").unwrap().unwrap(),
        b"UnregisteredDevice"
    );
}
#[test]
fn empty_persist_identity_skips_network_and_is_not_written() {
    let dir = tempfile::tempdir().unwrap();
    let comma = dir.path().join("comma");
    std::fs::create_dir(&comma).unwrap();
    for (name, bytes) in [
        ("dongle_id", " \u{1c}\n"),
        ("id_rsa", "invalid unused"),
        ("id_rsa.pub", "present"),
    ] {
        std::fs::write(comma.join(name), bytes).unwrap();
    }
    let params = Params::open(&dir.path().join("params"), "d").unwrap();
    let mut logger =
        openpilot_logging::producer::Factory::new(format!("ipc://{}/unused", dir.path().display()))
            .unwrap()
            .logger();
    let result = Registration {
        params: &params,
        persist: dir.path(),
        api_host: "http://127.0.0.1:1",
        source_root: Path::new("/unused"),
        logger: &mut logger,
    }
    .register(&mut Unused, &mut Unused, Some(&mut Unused))
    .unwrap();
    assert!(result.text_eq(""));
    assert!(params.get("DongleId").unwrap().is_none());
}
