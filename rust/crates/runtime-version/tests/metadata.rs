use openpilot_runtime_version::{
    build_metadata_from_dict, git::normalize_origin, Error, JsonValue,
};

#[test]
fn defaults_when_keys_are_missing() {
    // Given an older build file with no metadata keys.
    let input = JsonValue::parse("{}").unwrap();
    // When constructing metadata.
    let build = build_metadata_from_dict(&input).unwrap();
    // Then missing values default and canonical/UI use those values.
    assert_eq!(
        build.canonical().unwrap().to_utf8().unwrap(),
        "unknown-unknown-unknown"
    );
    assert_eq!(
        build.ui_description().unwrap().to_utf8().unwrap(),
        "unknown / unknow / unknown"
    );
    assert!(!build.openpilot.is_dirty);
}
#[test]
fn nonstring_fields_are_retained_until_used() {
    // Given source-accepted nonstring fields.
    let input = JsonValue::parse(r#"{"channel":null,"openpilot":{"version":42,"git_commit":true,"git_origin":[],"build_style":{"x":1}}}"#).unwrap();
    // When constructing metadata.
    let build = build_metadata_from_dict(&input).unwrap();
    // Then f-string formatting succeeds while string/slice operations fail at access time.
    assert_eq!(
        build.canonical().unwrap().to_utf8().unwrap(),
        "42-True-{'x': 1}"
    );
    assert!(matches!(
        build.openpilot.short_version(),
        Err(Error::Attribute(_))
    ));
    assert!(matches!(build.ui_description(), Err(Error::Type(_))));
}
#[test]
fn slice_counts_unicode_points_when_commit_is_non_ascii() {
    // Given six multibyte characters and a seventh.
    let input = JsonValue::parse(r#"{"openpilot":{"git_commit":"한글😀abcd"}}"#).unwrap();
    // When formatting the source UI property.
    let output = build_metadata_from_dict(&input)
        .unwrap()
        .ui_description()
        .unwrap();
    // Then slicing follows Python characters, not bytes.
    assert_eq!(output.to_utf8().unwrap(), "unknown / 한글😀abc / unknown");
}
#[test]
fn normalization_replaces_only_first_occurrence_anywhere() {
    // Given patterns repeated outside URL prefixes/suffixes.
    let input = "xgit@git@host.git.githttps://https://:tail";
    // When applying source normalization.
    let output = normalize_origin(input);
    // Then each operation replaces once in its inherited order.
    assert_eq!(output, "xgit@host.githttps///:tail");
}
