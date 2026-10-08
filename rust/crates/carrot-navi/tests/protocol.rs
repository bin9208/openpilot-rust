use openpilot_carrot_navi::{json::Value, manifest::MapConfig, packet};

#[test]
fn malformed_number_and_string_errors_keep_python_locations() {
    for (input, expected) in [
        (
            "{\"x\":1.}",
            "Expecting ',' delimiter: line 1 column 7 (char 6)",
        ),
        (
            "{\"x\":1e+}",
            "Expecting ',' delimiter: line 1 column 7 (char 6)",
        ),
        (
            "{\"x\":\"abc}",
            "Unterminated string starting at: line 1 column 6 (char 5)",
        ),
        (
            "{\"x\":\"\\q\"}",
            "Invalid \\escape: line 1 column 7 (char 6)",
        ),
        (
            "{\"x\":\"\\u000\"}",
            "Invalid \\uXXXX escape: line 1 column 8 (char 7)",
        ),
    ] {
        assert_eq!(Value::parse(input).unwrap_err().message, expected);
    }
}

#[test]
fn python_repr_keeps_printable_unicode_and_selects_quotes() {
    assert_eq!(Value::text("é😀").repr().unwrap(), "'é😀'");
    assert_eq!(Value::text("a'b").repr().unwrap(), "\"a'b\"");
    assert_eq!(Value::text("\u{200b}").repr().unwrap(), "'\\u200b'");
}

#[test]
fn surrogate_map_theme_fails_as_value_error_with_original_message() {
    let config = Value::object([("map_theme", Value::Text(vec![0xd800]))]);
    let error = MapConfig::parse(&config).unwrap_err();
    assert_eq!(error.kind, "ValueError");
    assert_eq!(
        error.message_value(),
        Value::Text(
            "unsupported map theme: "
                .chars()
                .map(u32::from)
                .chain([0xd800])
                .collect()
        )
    );
}

#[test]
fn missing_object_key_reports_source_json_error_and_character_location() {
    let rejected = Value::parse("{").unwrap_err();
    assert_eq!(rejected.kind, "JSONDecodeError");
    assert_eq!(
        rejected.message,
        "Expecting property name enclosed in double quotes: line 1 column 2 (char 1)"
    );
}

#[test]
fn image_payload_error_precedes_dimension_error_when_both_are_invalid() {
    let mut bytes = vec![0; 40];
    bytes[..4].copy_from_slice(b"CNV2");
    bytes[4..7].copy_from_slice(&[2, 1, 1]);
    bytes[8..12].copy_from_slice(&1_u32.to_be_bytes());
    bytes[12..16].copy_from_slice(&1_u32.to_be_bytes());
    let rejected = packet::parse(&bytes).unwrap_err();
    assert_eq!(rejected.kind, "ValueError");
    assert_eq!(rejected.message, "invalid v2 PNG payload");
}

#[test]
fn boolean_refresh_rate_is_accepted_by_source_integer_conversion() {
    let config = MapConfig::parse(&Value::object([("map_hz", Value::Bool(true))])).unwrap();
    let manifest = config.manifest("fixture", Value::integer(1));
    let Value::Array(streams) = manifest.get("streams") else {
        panic!("streams array missing")
    };
    assert!(streams
        .last()
        .unwrap()
        .get("params")
        .get("fps")
        .number_eq(1));
    assert!(
        streams
            .iter()
            .find(|stream| stream.get("name").text_eq("lane_top"))
            .unwrap()
            .get("enabled")
            == &Value::Bool(false)
    );
}

#[test]
fn python_json_preserves_large_integer_nonfinite_and_surrogate_values() {
    let input = "{\"integer\":1208925819614629174706176,\"nan\":NaN,\"text\":\"\\ud800\"}";
    let parsed = Value::parse(input).unwrap();
    let encoded = parsed.encode().unwrap();
    assert!(encoded.contains("1208925819614629174706176"));
    assert!(matches!(parsed.get("nan"), Value::Float(value) if value.is_nan()));
    assert_eq!(
        Value::parse(&encoded).unwrap().get("text"),
        &Value::Text(vec![0xd800])
    );
}
