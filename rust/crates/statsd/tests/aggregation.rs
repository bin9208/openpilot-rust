use openpilot_runtime_version::{build_metadata_from_dict, JsonValue};
use openpilot_statsd::{
    aggregation::{Flush, MetricEvent, Metrics, Tags},
    daemon::{Publication, Storage},
    Error,
};
fn tags() -> Tags {
    Tags::new(
        &build_metadata_from_dict(
            &JsonValue::parse(
                r#"{"openpilot":{"git_origin":"git@github.com:commaai/openpilot.git"}}"#,
            )
            .unwrap(),
        )
        .unwrap(),
        || Ok("pc".into()),
    )
    .unwrap()
}
fn flush() -> Flush {
    Flush {
        started: false,
        timestamp_ns: 123,
        dongle_id: None,
    }
}
#[test]
fn malformed_float_is_rejected_before_unknown_type_logging() {
    let mut metrics = Metrics::default();
    let event = metrics.ingest("x:bad|unknown");
    assert_eq!(event, MetricEvent::Malformed("x:bad|unknown"));
}
#[test]
fn extra_separators_keep_original_first_field_behavior() {
    let mut metrics = Metrics::default();
    let event = metrics.ingest("x:1:ignored|g|ignored");
    assert_eq!(event, MetricEvent::Accepted);
}
#[test]
fn rendering_clears_metrics_before_disk_errors() {
    let mut metrics = Metrics::default();
    metrics.ingest("x:1|g");
    let output = metrics.render(&tags(), &flush()).unwrap();
    let temporary = tempfile::tempdir().unwrap();
    let mut storage = Storage::new(temporary.path().join("absent"));
    let result = storage.publish(&output);
    assert!(matches!(result, Err(Error::Io(_))));
    assert!(metrics.render(&tags(), &flush()).unwrap().is_empty());
}
#[test]
fn failed_encoding_leaves_temporary_file() {
    let temporary = tempfile::tempdir().unwrap();
    let mut storage = Storage::new(temporary.path().to_owned());
    let result = storage.publish(&[0xd800]);
    assert!(matches!(result, Err(Error::Unicode)));
    assert_eq!(std::fs::read_dir(temporary.path()).unwrap().count(), 1);
}
#[test]
fn full_directory_is_reported_even_without_metrics() {
    let temporary = tempfile::tempdir().unwrap();
    for index in 0..openpilot_statsd::FILE_LIMIT {
        std::fs::create_dir(temporary.path().join(index.to_string())).unwrap();
    }
    let mut storage = Storage::new(temporary.path().to_owned());
    let result = storage.publish(&[]).unwrap();
    assert_eq!(result, Publication::Full);
}

#[test]
fn timestamp_uses_total_microsecond_division() {
    let instant = rustix::time::Timespec {
        tv_sec: 1,
        tv_nsec: 999_999_999,
    };
    let timestamp = openpilot_statsd::clock::timestamp_ns(instant).unwrap();
    assert_eq!(timestamp, 1_999_999_000);
}

#[test]
fn invalid_origin_fails_before_hardware_lookup() {
    let metadata = build_metadata_from_dict(
        &JsonValue::parse(r#"{"openpilot":{"git_origin":null}}"#).unwrap(),
    )
    .unwrap();
    let result = Tags::new(&metadata, || panic!("hardware must not be queried"));
    assert!(matches!(result, Err(Error::Metadata(_))));
}

#[test]
fn second_logging_failure_propagates_after_unknown_failure() {
    let mut attempts = Vec::new();
    let result = openpilot_statsd::events::report(
        MetricEvent::Unknown("unknown"),
        "x:1|unknown",
        |name, _| {
            attempts.push(name.to_owned());
            Err(Error::Zmq(zmq::Error::EINVAL))
        },
    );
    assert!(matches!(result, Err(Error::Zmq(zmq::Error::EINVAL))));
    assert_eq!(attempts, ["unknown metric type", "malformed metric"]);
}
