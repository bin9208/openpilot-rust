use openpilot_logging::{
    log_site, native::Logger, producer::Delivery, rate::RateLimit, record::Level,
};

#[test]
fn rate_strict_boundary_and_deferred_restart() {
    let mut rate = RateLimit::default();
    let sequence = [
        1,
        2,
        100_000_001,
        100_000_002,
        200_000_002,
        300_000_002,
        300_000_003,
    ];
    let actual: Vec<_> = sequence
        .into_iter()
        .map(|ts| rate.admit(ts).unwrap())
        .collect();
    assert_eq!(
        actual
            .iter()
            .map(|v| (v.emit, v.suppressed))
            .collect::<Vec<_>>(),
        [
            (true, 0),
            (true, 0),
            (false, 0),
            (true, 1),
            (true, 0),
            (false, 0),
            (true, 1)
        ]
    );
}

#[test]
fn empty_text_does_not_initialize_transport() {
    let logger = Logger::new("invalid://endpoint".into(), "test-version", "pc").unwrap();
    assert_eq!(
        logger
            .emit(log_site!(), Level::Error, String::new())
            .unwrap(),
        Delivery::Filtered
    );
    assert!(logger
        .emit(log_site!(), Level::Error, "nonempty".into())
        .is_err());
}

#[test]
fn native_logger_can_cross_threads() {
    fn shared<T: Send + Sync>() {}
    shared::<Logger>();
}

#[test]
fn close_releases_the_socket_shared_by_all_clones() {
    let directory = tempfile::tempdir().unwrap();
    let endpoint = format!("ipc://{}", directory.path().join("log").display());
    let context = zmq::Context::new();
    let pull = context.socket(zmq::PULL).unwrap();
    pull.set_rcvtimeo(2000).unwrap();
    pull.bind(&endpoint).unwrap();
    let logger = Logger::new(endpoint, "test-version", "pc").unwrap();
    let clone = logger.clone();
    logger.close().unwrap();
    assert_eq!(
        clone
            .emit(log_site!(), Level::Debug, "ready".into())
            .unwrap(),
        Delivery::Sent
    );
    assert_eq!(pull.recv_bytes(0).unwrap()[0], 10);
    logger.close().unwrap();
    assert!(matches!(
        clone.emit(log_site!(), Level::Info, "closed".into()),
        Err(openpilot_logging::Error::Transport(zmq::Error::ENOTSOCK))
    ));
    clone.close().unwrap();
}

#[test]
fn virtual_source_labels_preserve_message_bytes_and_rust_callsite() {
    let directory = tempfile::tempdir().unwrap();
    let endpoint = format!("ipc://{}", directory.path().join("named").display());
    let context = zmq::Context::new();
    let pull = context.socket(zmq::PULL).unwrap();
    pull.set_rcvtimeo(2000).unwrap();
    pull.bind(&endpoint).unwrap();
    let logger = Logger::new(endpoint, "test-version", "pc").unwrap();
    let site = log_site!();
    logger
        .emit_named(
            site,
            "panda[1]",
            Level::Warning,
            "SPI: test\0ignored".into(),
        )
        .unwrap();
    let packet = pull.recv_bytes(0).unwrap();
    let record: serde_json::Value = serde_json::from_slice(&packet[1..]).unwrap();
    assert_eq!(record["filename"], "panda[1]");
    assert_eq!(record["msg"], "SPI: test");
    assert_eq!(record["lineno"], site.line);
    assert_eq!(record["funcname"], site.function);
    logger.close().unwrap();
}

#[test]
fn timestamp_messages_use_string_times_and_optional_source_frame_id() {
    let directory = tempfile::tempdir().unwrap();
    let endpoint = format!("ipc://{}", directory.path().join("timestamps").display());
    let context = zmq::Context::new();
    let pull = context.socket(zmq::PULL).unwrap();
    pull.set_rcvtimeo(2000).unwrap();
    pull.bind(&endpoint).unwrap();
    let logger = Logger::new(endpoint, "test-version", "pc").unwrap();
    for frame in [None, Some(17), Some(u32::MAX)] {
        logger
            .emit_timestamp(log_site!(), Level::Debug, "sendcan sent".into(), frame)
            .unwrap();
        let packet = pull.recv_bytes(0).unwrap();
        let record: serde_json::Value = serde_json::from_slice(&packet[1..]).unwrap();
        let timestamp = &record["msg"]["timestamp"];
        assert_eq!(timestamp["event"], "sendcan sent");
        assert!(timestamp["time"].as_str().unwrap().parse::<u64>().unwrap() > 0);
        assert_eq!(
            timestamp.get("frame_id"),
            (frame == Some(17)).then_some(&serde_json::json!("17"))
        );
    }
    logger.close().unwrap();
}
