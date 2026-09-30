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
