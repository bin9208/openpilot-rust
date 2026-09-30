use openpilot_journald::{decode, packet, parse_line, Error};

#[test]
fn ordered_message_retains_duplicates_numbers_and_python_strings() {
    let entry = parse_line(r#"{"z":1,"a":184467440737095516160,"z":2,"s":"\ud800","n":NaN}"#)
        .unwrap()
        .unwrap();
    assert_eq!(
        entry.message,
        r#"{"z": 2, "a": 184467440737095516160, "s": "\ud800", "n": NaN}"#
    );
}

#[test]
fn invalid_json_is_recoverable_but_field_conversion_is_fatal() {
    for input in [
        "{bad",
        "{\"x\":\"literal\u{1}\"}",
        "{\"literal\u{1}\":1}",
        "{\"x\":[1,]}",
    ] {
        assert!(matches!(
            parse_line(input),
            Err(Error::Json(decode::Error::Syntax { .. }))
        ));
    }
    for input in [
        r#"{"_PID":"\u001c1"}"#,
        r#"{"_PID":null}"#,
        r#"{"PRIORITY":256}"#,
        r#"{"SYSLOG_IDENTIFIER":"\ud800"}"#,
    ] {
        assert!(matches!(parse_line(input), Err(Error::Field(_))));
    }
    assert!(matches!(parse_line("[]"), Err(Error::Root)));
}

#[test]
fn integers_follow_python_conversions_and_wire_ranges() {
    let entry = parse_line(r#"{"_PID":" -١_٢ ","PRIORITY":true,"__REALTIME_TIMESTAMP":12.8}"#)
        .unwrap()
        .unwrap();
    assert_eq!((entry.pid, entry.priority, entry.timestamp), (-12, 1, 12));
    let maximum = parse_line(r#"{"__REALTIME_TIMESTAMP":18446744073709551615}"#)
        .unwrap()
        .unwrap();
    assert_eq!(maximum.timestamp, u64::MAX);
    assert!(parse_line(r#"{"__REALTIME_TIMESTAMP":18446744073709551616}"#).is_err());
}

#[test]
fn empty_line_and_absent_tag_preserve_source_message_presence() {
    assert!(parse_line(" \u{1c}\t\r\n").unwrap().is_none());
    let entry = parse_line("{}").unwrap().unwrap();
    let bytes = packet(&entry, 123);
    let reader =
        capnp::serialize::read_message(&bytes[..], capnp::message::ReaderOptions::new()).unwrap();
    let event = reader
        .get_root::<openpilot_cereal::log_capnp::event::Reader>()
        .unwrap();
    assert!(!event.get_valid());
    assert_eq!(event.get_log_mono_time(), 123);
    let openpilot_cereal::log_capnp::event::Which::AndroidLog(entry) = event.which().unwrap()
    else {
        panic!("androidLog expected");
    };
    assert!(!entry.unwrap().has_tag());
}

#[test]
fn excessive_integer_is_fatal_instead_of_malformed_json() {
    let input = format!("{{\"x\":{}}}", "1".repeat(4301));
    assert!(matches!(
        parse_line(&input),
        Err(Error::Json(decode::Error::IntegerLimit))
    ));
}

#[test]
fn source_valid_deep_json_does_not_depend_on_rust_call_stack() {
    let nested = format!("{}0{}", "[".repeat(5000), "]".repeat(5000));
    let input = format!("{{\"x\":{nested}}}");
    let entry = parse_line(&input).unwrap().unwrap();
    assert_eq!(entry.message, format!("{{\"x\": {nested}}}"));
}
