use openpilot_logmessaged::JsonValue;
use openpilot_tombstoned::daemon::crash_filename;

#[test]
fn truncates_codepoints_after_composition_when_path_is_unicode() {
    // Given a long multibyte safe path and a source-style commit.
    let path = "한".repeat(100);
    // When constructing the copy destination basename.
    let name = crash_filename(
        "2020-01-02--03-04-05",
        &JsonValue::text("abcdefghijk"),
        &path,
    )
    .unwrap();
    // Then the complete filename has 62 codepoints and an eight-character commit prefix.
    assert_eq!(name.chars().count(), 62);
    assert!(name.starts_with("2020-01-02--03-04-05_abcdefgh_"));
    assert!(name.ends_with('한'));
}
#[test]
fn keeps_source_falsey_and_nonstring_slice_policy() {
    // Given an empty commit and a list-valued build commit.
    let empty = JsonValue::parse("null").unwrap();
    let list = JsonValue::parse("[true,17]").unwrap();
    // When composing both accepted source values.
    let output = [
        crash_filename("date", &empty, "a/b").unwrap(),
        crash_filename("date", &list, "a/b").unwrap(),
    ];
    // Then only falsey values use nocommit; lists retain Python formatting.
    assert_eq!(output, ["date_nocommit_a_b", "date_[True, 17]_a_b"]);
}
