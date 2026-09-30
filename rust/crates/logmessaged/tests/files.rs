use openpilot_logmessaged::{format_record, LogFiles, RotationSettings};
use std::{cell::Cell, fs};
use uuid::Uuid;

#[test]
fn formatter_preserves_types_order_ascii_and_nested_list_contents() {
    let formatted = format_record(r#"{"msg":{"s":"한😀","f":0.000001,"b":true,"i":123456789012345678901234567890,"nil":null,"a":[{"x":1}],"d":{"n":1}},"ctx":{},"id":"old"}"#, Uuid::nil()).unwrap();
    assert_eq!(
        formatted,
        r#"{"ctx": {}, "id": "00000000000000000000000000000000", "msg": {"s$s": "\ud55c\ud83d\ude00", "f$f": 1e-06, "b$b": true, "i$i": 123456789012345678901234567890, "nil": null, "a$a": [{"x": 1}], "d": {"n$i": 1}}}"#
    );
}

#[test]
fn formatter_keeps_unpaired_surrogates_and_nonfinite_json() {
    let formatted = format_record(
        r#"{"msg":{"a":"\ud800","b":NaN,"c":Infinity,"d":-Infinity}}"#,
        Uuid::nil(),
    )
    .unwrap();
    assert!(formatted.contains(r#""a$s": "\ud800", "b$f": NaN, "c$f": Infinity, "d$f": -Infinity"#));
    for invalid in ["{", "{}", "[]", "{\"msg\":01}"] {
        assert!(format_record(invalid, Uuid::nil()).is_err());
    }
}

#[test]
fn startup_retention_keeps_source_ascending_order_quirk() {
    let directory = tempfile::tempdir().unwrap();
    for name in [
        "swaglog.0000000001",
        "swaglog.0000000002",
        "swaglog.0000000003",
        "swaglog.note",
    ] {
        fs::write(directory.path().join(name), b"old\n").unwrap();
    }
    let settings = RotationSettings {
        backup_count: 2,
        ..RotationSettings::default()
    };
    let handler = LogFiles::new(&directory.path().join("swaglog"), settings, || 0.).unwrap();
    assert!(directory.path().join("swaglog.0000000001").exists());
    assert!(directory.path().join("swaglog.0000000004").exists());
    assert!(!directory.path().join("swaglog.0000000003").exists());
    drop(handler);
}

#[test]
fn rotation_checks_existing_bytes_and_time_before_formatting() {
    let directory = tempfile::tempdir().unwrap();
    let settings = RotationSettings {
        max_bytes: 1,
        interval: 60.,
        backup_count: 2500,
    };
    let clock = Cell::new(0.);
    let mut handler =
        LogFiles::new(&directory.path().join("swaglog"), settings, || clock.get()).unwrap();
    handler.emit(r#"{"msg":"first"}"#).unwrap();
    assert!(
        fs::metadata(directory.path().join("swaglog.0000000000"))
            .unwrap()
            .len()
            > 1
    );
    clock.set(1.);
    assert!(handler.emit("not JSON").is_err());
    assert_eq!(
        fs::metadata(directory.path().join("swaglog.0000000001"))
            .unwrap()
            .len(),
        0
    );
    clock.set(61.);
    handler.emit(r#"{"msg":"at time boundary"}"#).unwrap();
    assert!(
        fs::metadata(directory.path().join("swaglog.0000000002"))
            .unwrap()
            .len()
            > 0
    );
}

#[test]
fn renamed_nested_keys_overwrite_in_place_like_python_dicts() {
    let formatted = format_record(r#"{"msg":{"x":1,"y":true,"x$i":null}}"#, Uuid::nil()).unwrap();
    assert_eq!(
        formatted,
        r#"{"msg": {"x$i": null, "y$b": true}, "id": "00000000000000000000000000000000"}"#
    );
}

#[test]
fn existing_invalid_utf8_paths_use_python_surrogateescape_sort_order() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let directory = tempfile::tempdir().unwrap();
    let invalid = directory
        .path()
        .join(OsString::from_vec(b"swaglog.\xff".to_vec()));
    let unicode = directory.path().join("swaglog.\u{e000}");
    fs::write(&invalid, b"invalid name").unwrap();
    fs::write(&unicode, b"unicode name").unwrap();
    let settings = RotationSettings {
        backup_count: 2,
        ..RotationSettings::default()
    };
    let _handler = LogFiles::new(&directory.path().join("swaglog"), settings, || 0.).unwrap();
    assert!(invalid.exists());
    assert!(!unicode.exists());
}

#[test]
fn float_decimal_ties_match_python_even_rounding() {
    let formatted = format_record(r#"{"msg":181449526438435.12}"#, Uuid::nil()).unwrap();
    assert!(formatted.contains("181449526438435.12"), "{formatted}");
}

#[test]
fn buffered_disk_error_is_reported_again_on_close() {
    let directory = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink("/dev/full", directory.path().join("swaglog.0000000000")).unwrap();
    let mut files = LogFiles::new(
        &directory.path().join("swaglog"),
        RotationSettings::default(),
        || 0.,
    )
    .unwrap();
    assert!(files.emit(r#"{"msg":"disk full"}"#).is_err());
    assert!(files.close().is_err());
}

#[test]
fn deep_json_uses_heap_worklists_through_format_and_destruction() {
    let depth = 5000;
    for (open, close) in [("[", "]"), ("{\"a\":", "}")] {
        let record = format!("{{\"msg\":{}0{}}}", open.repeat(depth), close.repeat(depth));
        let formatted = format_record(&record, Uuid::nil()).unwrap();
        if open == "[" {
            assert_eq!(formatted.matches('[').count(), depth);
            assert!(formatted.starts_with("{\"msg$a\": "));
        } else {
            assert!(formatted.contains("\"a$i\": 0"));
            assert_eq!(formatted.matches("\"a\":").count(), depth - 1);
        }
        drop(formatted);
    }
}

#[test]
fn rotation_checks_and_file_opens_sample_clock_separately() {
    let directory = tempfile::tempdir().unwrap();
    let mut reads = [0., 60., 61., 120.].into_iter();
    let mut handler = LogFiles::new(
        &directory.path().join("swaglog"),
        RotationSettings::default(),
        || reads.next().expect("exact source clock reads"),
    )
    .unwrap();
    handler.emit(r#"{"msg":"first"}"#).unwrap();
    handler.emit(r#"{"msg":"second"}"#).unwrap();
    handler.close().unwrap();
    assert!(!directory.path().join("swaglog.0000000002").exists());
    assert_eq!(
        fs::read_to_string(directory.path().join("swaglog.0000000001"))
            .unwrap()
            .lines()
            .count(),
        2
    );
}
