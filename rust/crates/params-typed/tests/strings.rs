use openpilot_logging::producer::Factory;
use openpilot_params::Params;
use openpilot_params_typed::{get_string, Error};

#[test]
fn string_reads_distinguish_values_from_missing_defaults_and_validate_keys() {
    let directory = tempfile::tempdir().unwrap();
    let params = Params::open(directory.path(), "d").unwrap();
    let mut logger = Factory::new(format!("ipc://{}/unused", directory.path().display()))
        .unwrap()
        .logger();
    assert_eq!(
        get_string(&params, "CompletedTrainingVersion", &mut logger).unwrap(),
        None
    );
    params.put("DongleId", b"").unwrap();
    assert_eq!(get_string(&params, "DongleId", &mut logger).unwrap(), None);
    params.put("DongleId", "한글\0value".as_bytes()).unwrap();
    assert_eq!(
        get_string(&params, "DongleId", &mut logger)
            .unwrap()
            .as_deref(),
        Some("한글\0value")
    );
    assert!(matches!(
        get_string(&params, "NotARegisteredKey", &mut logger),
        Err(Error::Params(openpilot_params::Error::UnknownKey(_)))
    ));
    assert!(matches!(
        get_string(&params, "IsOffroad", &mut logger),
        Err(Error::NotString { .. })
    ));
}

#[test]
fn invalid_string_emits_the_source_warning_and_propagates_closed_logger_errors() {
    let directory = tempfile::tempdir().unwrap();
    let params = Params::open(directory.path(), "d").unwrap();
    params.put("DongleId", b"one'\xff").unwrap();
    let context = zmq::Context::new();
    let receiver = context.socket(zmq::PULL).unwrap();
    receiver.set_rcvtimeo(5000).unwrap();
    let endpoint = format!("ipc://{}/logs", directory.path().display());
    receiver.bind(&endpoint).unwrap();
    let mut logger = Factory::new(endpoint).unwrap().logger();
    assert_eq!(get_string(&params, "DongleId", &mut logger).unwrap(), None);
    let bytes = receiver.recv_bytes(0).unwrap();
    assert_eq!(bytes[0], 30);
    let record: serde_json::Value = serde_json::from_slice(&bytes[1..]).unwrap();
    assert_eq!(record["msg"], "Failed to cast param DongleId with value=b\"one'\\xff\" from type t=<ParamKeyType.STRING: 0>");
    assert_eq!(record["level"], "WARNING");
    assert_eq!(record["filename"], "lib.rs");
    assert_eq!(record["funcName"], "openpilot_params_typed::get_string");
    logger.close();
    assert!(matches!(
        get_string(&params, "DongleId", &mut logger),
        Err(Error::Logging(_))
    ));
    assert!(matches!(
        receiver.recv_bytes(zmq::DONTWAIT),
        Err(zmq::Error::EAGAIN)
    ));
}
