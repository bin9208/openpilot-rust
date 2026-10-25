//! Independent pinned external SDK fields; the native candidate never supplies expected data.
use openpilot_dev_tools::sdk_transport_check::{expected, verify, verify_dirty};
use openpilot_log_format::{JsonValue as J, JsonView};
fn candidate(index: usize) -> J {
    let source = expected(index).unwrap().to_json().unwrap();
    J::parse(&format!(
        "{{{},\"platform\":\"native\",\"sdk\":{{\"name\":\"sentry.rust\"}}}}",
        &source[1..source.len() - 1]
    ))
    .unwrap()
}
#[test]
fn thirteen_external_sdk_cases_preserve_selected_fields_losslessly() {
    for index in 0..13 {
        verify(
            index,
            &candidate(index),
            Some("Sentry sentry_key=fixture, sentry_version=7"),
        )
        .unwrap();
    }
    let integer = expected(2)
        .unwrap()
        .get("tags")
        .unwrap()
        .get("unusual")
        .unwrap();
    assert!(matches!(integer.view(), JsonView::Integer(v) if v.len() == 101));
    assert!(
        matches!(expected(11).unwrap().get("tags").unwrap().get("unusual").unwrap().view(), JsonView::Text(v) if v == [0xd800])
    );
    assert!(expected(14).is_err());
}
#[test]
fn mismatch_does_not_silently_coerce_boolean_drop_meta_or_accept_wrong_sdk() {
    let original = candidate(0).to_json().unwrap();
    for value in [
        original.replacen("\"dirty\": false", "\"dirty\": 0", 1),
        original.replacen("\"_meta\":", "\"ignored_meta\":", 1),
        original.replacen("sentry.rust", "sentry.python", 1),
        original.replacen("\"platform\": \"native\"", "\"platform\": \"python\"", 1),
    ] {
        assert!(value != original, "negative control did not mutate event");
        assert!(verify(0, &J::parse(&value).unwrap(), Some("sentry_key=fixture")).is_err());
    }
    assert!(verify(0, &candidate(0), None).is_err());
    assert!(verify(0, &candidate(0), Some("sentry_key=wrong")).is_err());
}
#[test]
fn dirty_commit_comes_from_current_independent_source_calls() {
    let original = candidate(13).to_json().unwrap();
    let old = expected(13)
        .unwrap()
        .get("tags")
        .unwrap()
        .get("commit")
        .unwrap()
        .to_utf8()
        .unwrap();
    let current = "1234567890123456789012345678901234567890";
    let event = J::parse(&original.replace(&old, current)).unwrap();
    let calls = J::parse(&format!("[{{\"op\":\"set_tag\",\"fields\":{{\"key\":\"commit\",\"value_json\":\"\\\"{current}\\\"\"}}}}]")).unwrap();
    verify_dirty(&event, &calls).unwrap();
    assert!(verify_dirty(&candidate(13), &calls).is_err());
    assert!(verify_dirty(&event, &J::parse("[]").unwrap()).is_err());
}
