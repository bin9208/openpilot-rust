use openpilot_logmessaged::string_fields;

#[test]
fn selected_fields_keep_last_duplicate_value_and_ignore_nonfinite_extras() {
    let input = r#"{"status":null,"status":"success","timezone":"bad","timezone":"Asia/Seoul","extra":[NaN,Infinity,-Infinity,"\ud800"]}"#;
    let fields = string_fields(input, ["status", "timezone"]).unwrap();
    assert_eq!(fields, [Some("success".into()), Some("Asia/Seoul".into())]);
}

#[test]
fn last_nonstring_or_surrogate_field_is_absent() {
    let input = r#"{"status":"success","status":false,"timezone":"\ud800"}"#;
    let fields = string_fields(input, ["status", "timezone"]).unwrap();
    assert_eq!(fields, [None, None]);
}

#[test]
fn malformed_unknown_value_rejects_the_whole_document() {
    let input = r#"{"status":"success","extra":NaNgarbage}"#;
    assert!(string_fields(input, ["status"]).is_err());
}
