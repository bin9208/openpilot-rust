use openpilot_logmessaged::{JsonValue, JsonView};
#[test]
fn immutable_child_survives_document_drop_and_retains_unusual_values() {
    // Given duplicate fields, an arbitrary integer and a lone surrogate.
    let document =
        JsonValue::parse(r#"{"x":0,"x":[123456789012345678901234567890,"\ud800",NaN]}"#).unwrap();
    // When obtaining a child and dropping the root.
    let child = document.get("x").unwrap();
    drop(document);
    // Then serialization is source-compatible and the parser's arena remains alive.
    assert_eq!(
        child.to_json().unwrap(),
        r#"[123456789012345678901234567890, "\ud800", NaN]"#
    );
    let JsonView::Array(values) = child.view() else {
        panic!("array expected")
    };
    assert!(values[1].to_utf8().is_none());
}
#[test]
fn unicode_constructor_checks_range_but_accepts_python_surrogates() {
    // Given Python codepoints and an invalid codepoint.
    let valid = vec![0xd800];
    let invalid = vec![0x110000];
    // When constructing immutable text values.
    let value = JsonValue::codepoints(valid).unwrap();
    let rejected = JsonValue::codepoints(invalid);
    // Then no replacement decoding occurs and invalid codepoints are rejected.
    assert_eq!(value.to_json().unwrap(), r#""\ud800""#);
    assert!(rejected.is_none());
}
