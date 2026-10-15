use openpilot_log_format::{JsonValue, JsonView};
#[test]
fn raw_surrogates_stay_distinct_from_escaped_pairs_and_object_keys() {
    let mut points = "{\"".chars().map(u32::from).collect::<Vec<_>>();
    points.extend([0xd800, 0xdc00]);
    points.extend("\": 1, \"\\ud800\\udc00\": 2}".chars().map(u32::from));
    let value = JsonValue::parse_codepoints(&points).unwrap();
    let JsonView::Object(fields) = value.view() else {
        panic!("object expected")
    };
    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0].0, [0xd800, 0xdc00]);
    assert_eq!(fields[1].0, [0x10000]);
    let ordinary = JsonValue::parse("\"\\ud800\\udc00\"").unwrap();
    let JsonView::Text(text) = ordinary.view() else {
        panic!("text expected")
    };
    assert_eq!(text, [0x10000]);
}
#[test]
fn raw_markers_do_not_bypass_json_syntax_or_escape_validation() {
    for points in [
        vec![0xd800],
        vec![34, 92, 0xd800, 34],
        vec![0x110000],
        vec![34, 0, 34],
    ] {
        assert!(JsonValue::parse_codepoints(&points).is_err());
    }
    for points in [
        vec![34, 0xd800, 92, 117, 100, 99, 48, 48, 34],
        vec![34, 92, 117, 100, 56, 48, 48, 0xdc00, 34],
    ] {
        let value = JsonValue::parse_codepoints(&points).unwrap();
        let JsonView::Text(text) = value.view() else {
            panic!("text expected")
        };
        assert_eq!(text, [0xd800, 0xdc00]);
    }
}
