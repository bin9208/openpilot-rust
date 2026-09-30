use openpilot_cereal::log_capnp::{event, init_data::DeviceType, sentinel::SentinelType};
use openpilot_loggerd::{
    metadata::Environment,
    writer::{route_name, Logger},
};
use openpilot_params::Params;
use std::{fs, io::Cursor, os::unix::fs::symlink, path::Path};

fn init() -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    message
        .init_root::<event::Builder>()
        .init_init_data()
        .set_version("fixture");
    capnp::serialize::write_message_to_words(&message)
}

fn log_contents(path: &Path) -> Vec<(String, i32)> {
    let bytes = zstd::decode_all(fs::File::open(path).unwrap()).unwrap();
    let mut reader = Cursor::new(bytes);
    let mut result = Vec::new();
    while let Some(message) =
        capnp::serialize::try_read_message(&mut reader, Default::default()).unwrap()
    {
        let event = message.get_root::<event::Reader>().unwrap();
        result.push(match event.which().unwrap() {
            event::InitData(_) => ("init".into(), 0),
            event::LogMessage(value) => (value.unwrap().to_str().unwrap().into(), 0),
            event::Sentinel(value) => {
                let value = value.unwrap();
                (
                    format!("{:?}", value.get_type().unwrap()),
                    value.get_signal(),
                )
            }
            _ => panic!("unexpected fixture event"),
        });
    }
    result
}

#[test]
fn emits_segment_and_route_sentinels_when_rotated_and_closed() {
    let root = tempfile::tempdir().unwrap();
    let mut logger = Logger::new(root.path(), "route".into(), init());
    logger.next(10).unwrap();
    logger.next(20).unwrap();
    logger.close(15, 30).unwrap();
    assert_eq!(
        log_contents(&root.path().join("route--0/rlog.zst")),
        vec![
            ("init".into(), 0),
            ("StartOfRoute".into(), 0),
            ("EndOfSegment".into(), 0)
        ]
    );
    assert_eq!(
        log_contents(&root.path().join("route--1/qlog.zst")),
        vec![
            ("init".into(), 0),
            ("StartOfSegment".into(), 0),
            ("EndOfRoute".into(), 15)
        ]
    );
    assert!(!root.path().join("route--0/rlog.lock").exists());
    assert!(!root.path().join("route--1/rlog.lock").exists());
}

#[test]
fn retains_incomplete_lock_when_compression_output_fails() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("route--0")).unwrap();
    symlink("/dev/full", root.path().join("route--0/rlog.zst")).unwrap();
    let mut logger = Logger::new(root.path(), "route".into(), init());
    logger.next(10).unwrap();
    let result = logger.close(15, 20);
    assert!(result.is_err());
    assert!(root.path().join("route--0/rlog.lock").exists());
}

#[test]
fn appends_route_once_per_segment_when_preservation_is_requested() {
    let root = tempfile::tempdir().unwrap();
    let params = Params::open(&root.path().join("params"), "d").unwrap();
    params
        .put("AthenadRecentlyViewedRoutes", b"existing")
        .unwrap();
    let mut logger = Logger::new(root.path(), "route".into(), init());
    logger.next(10).unwrap();
    logger.preserve(&params).unwrap();
    logger.preserve(&params).unwrap();
    logger.next(20).unwrap();
    logger.preserve(&params).unwrap();
    assert_eq!(
        params.get("AthenadRecentlyViewedRoutes").unwrap().unwrap(),
        b"existing,route,route"
    );
    let mut value = [0_u8; 1];
    assert_eq!(
        rustix::fs::getxattr(root.path().join("route--0"), "user.preserve", &mut value).unwrap(),
        1
    );
    assert_eq!(value, *b"1");
}

#[test]
fn redacts_sensitive_params_without_replacing_vehicle_identity() {
    let root = tempfile::tempdir().unwrap();
    let params = Params::open(root.path(), "d").unwrap();
    params
        .put("DongleId", b"original-fixture-identity")
        .unwrap();
    params.put("AccessToken", b"private-fixture").unwrap();
    let environment = Environment {
        log_root: root.path().join("logs"),
        params_root: root.path().to_owned(),
        prefix: "d".into(),
        device: DeviceType::Pc,
    };
    let bytes = environment.init_data().unwrap();
    let message =
        capnp::serialize::read_message_from_flat_slice(&mut bytes.as_slice(), Default::default())
            .unwrap();
    let event::InitData(init) = message
        .get_root::<event::Reader>()
        .unwrap()
        .which()
        .unwrap()
    else {
        panic!("missing init");
    };
    let init = init.unwrap();
    assert_eq!(
        init.get_dongle_id().unwrap().to_str().unwrap(),
        "original-fixture-identity"
    );
    let params: std::collections::BTreeMap<_, _> = init
        .get_params()
        .unwrap()
        .get_entries()
        .unwrap()
        .iter()
        .map(|entry| {
            (
                entry.get_key().unwrap().to_str().unwrap().to_owned(),
                entry.get_value().unwrap().to_vec(),
            )
        })
        .collect();
    assert_eq!(params["AccessToken"], b"");
    assert_eq!(params["GitCommit"], b"");
}

#[test]
fn wraps_native_unsigned_counter_when_saved_count_is_negative() {
    let root = tempfile::tempdir().unwrap();
    let params = Params::open(root.path(), "d").unwrap();
    params.put("RouteCount", b" -1suffix").unwrap();
    let route = route_name(&params).unwrap();
    assert!(route.starts_with("ffffffff--"), "{route}");
    assert_eq!(params.get("RouteCount").unwrap().unwrap(), b"0");
}

#[test]
fn stores_ordinary_message_bytes_without_reserialization() {
    let root = tempfile::tempdir().unwrap();
    let mut logger = Logger::new(root.path(), "route".into(), init());
    logger.next(10).unwrap();
    let mut message = capnp::message::Builder::new_default();
    message
        .init_root::<event::Builder>()
        .set_log_message("original payload");
    let bytes = capnp::serialize::write_message_to_words(&message);
    logger.write(&bytes, false).unwrap();
    logger.close(0, 30).unwrap();
    let rlog =
        zstd::decode_all(fs::File::open(root.path().join("route--0/rlog.zst")).unwrap()).unwrap();
    assert!(rlog.windows(bytes.len()).any(|window| window == bytes));
    assert_eq!(
        log_contents(&root.path().join("route--0/qlog.zst")),
        vec![
            ("init".into(), 0),
            (format!("{:?}", SentinelType::StartOfRoute), 0),
            ("EndOfRoute".into(), 0)
        ]
    );
}
