use openpilot_logging::{
    log_site,
    producer::{Delivery, Factory, Logger},
    record::{Level, Record},
};
use openpilot_uploader::{
    runtime::RuntimeEvents, EventSink, Outcome, Transfer, TransferError, UploadResponse, Uploader,
    XattrCache,
};
use std::{fs, path::Path};

struct NoTransfer;
impl Transfer for NoTransfer {
    fn upload(
        &mut self,
        _: &Path,
        _: &Path,
        _: &mut dyn EventSink,
    ) -> Result<UploadResponse, TransferError> {
        panic!("log failure or empty file must not start a transfer");
    }
}
fn saturated(endpoint: String) -> Logger {
    let mut logger = Factory::new(endpoint).unwrap().logger();
    for _ in 0..10_000 {
        if logger
            .emit(
                log_site!(),
                Record::text(Level::Debug, "queue fixture".into()),
            )
            .unwrap()
            == Delivery::Dropped
        {
            return logger;
        }
    }
    panic!("disconnected ZMQ queue did not reach EAGAIN");
}
#[test]
fn upload_start_logging_failure_stops_before_marking_an_empty_file() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("route--0")).unwrap();
    let file = root.path().join("route--0/qlog");
    fs::write(&file, b"").unwrap();
    let mut logger = saturated(format!("ipc://{}/unbound", root.path().display()));
    logger.close();
    let mut uploader = Uploader::new(
        root.path().into(),
        NoTransfer,
        XattrCache::default(),
        RuntimeEvents::new(logger),
    );
    assert!(uploader.step(1, false, None).is_err());
    assert_eq!(
        rustix::fs::getxattr(&file, "user.upload", &mut [0_u8; 8]),
        Err(rustix::io::Errno::NODATA)
    );
}
#[test]
fn scan_logging_failure_propagates_instead_of_skipping_the_broken_entry() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("route--0")).unwrap();
    std::os::unix::fs::symlink("absent", root.path().join("route--0/qlog")).unwrap();
    let mut logger = saturated(format!("ipc://{}/unbound", root.path().display()));
    logger.close();
    let mut uploader = Uploader::new(
        root.path().into(),
        NoTransfer,
        XattrCache::default(),
        RuntimeEvents::new(logger),
    );
    assert!(uploader.step(1, false, None).is_err());
}
#[test]
fn actual_eagain_keeps_upload_logging_nonfatal() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("route--0")).unwrap();
    let file = root.path().join("route--0/qlog");
    fs::write(&file, b"").unwrap();
    let logger = saturated(format!("ipc://{}/unbound", root.path().display()));
    let mut uploader = Uploader::new(
        root.path().into(),
        NoTransfer,
        XattrCache::default(),
        RuntimeEvents::new(logger),
    );
    assert_eq!(uploader.step(1, false, None).unwrap(), Outcome::Success);
    let mut value = [0_u8; 8];
    let length = rustix::fs::getxattr(&file, "user.upload", &mut value).unwrap();
    assert_eq!(&value[..length], b"1");
}

#[test]
fn record_conversion_error_is_not_suppressed_by_the_runtime_sink() {
    let root = tempfile::tempdir().unwrap();
    let logger = Factory::new(format!("ipc://{}/unused", root.path().display()))
        .unwrap()
        .logger();
    let mut sink = RuntimeEvents::new(logger);
    let error = sink
        .emit(openpilot_uploader::Event::Fields {
            site: log_site!(),
            name: "invalid_fixture",
            fields: serde_json::Value::Null,
        })
        .unwrap_err();
    assert!(matches!(
        error,
        openpilot_logging::Error::Contract("uploader event fields must be an object")
    ));
}
