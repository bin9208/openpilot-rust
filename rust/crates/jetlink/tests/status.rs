use openpilot_jetlink::status;
use serde_json::{json, Value};
#[test]
fn original_display_diagnostics_and_badge_match6048_source_cases() {
    let fixture: Value = serde_json::from_str(include_str!("status_source_vectors.json")).unwrap();
    assert_eq!(
        fixture["source_commit"],
        "e8e152dbcfd9574a1791ed12026d1880e87689b3"
    );
    assert_eq!(
        fixture["source_sha256"],
        "e97491c814648e090b4bfbe9c6672410e452558fd5cf261def97bd1a1c81bfe4"
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 6048);
    for (index, case) in cases.iter().enumerate() {
        assert_eq!(
            serde_json::to_value(status::diagnostics(
                &case["saved"],
                &case["link"],
                &case["model"],
                10.
            ))
            .unwrap(),
            case["expected"],
            "diagnostics case{index}"
        );
        let badge = status::badge(&case["saved"], &case["link"], &case["model"], 10.);
        let badge = badge.map(|value| json!([value.text, value.style]));
        assert_eq!(
            serde_json::to_value(badge).unwrap(),
            case["badge"],
            "badge case{index}"
        );
    }
}
#[test]
fn recovery_and_delivery_badges_and_strict_status_age() {
    for (state, text, style) in [
        ("waiting", "eGPU RETRY", "loading"),
        ("recovering", "eGPU RETRY", "loading"),
        ("ready", "eGPU READY", "ready"),
        ("stopped", "eGPU ERROR", "error"),
    ] {
        let badge = status::recovery_badge(&json!({"state":state})).unwrap();
        assert_eq!((badge.text.as_str(), badge.style.as_str()), (text, style));
    }
    for (state, error, text) in [
        ("waiting_for_network", "clock", "eGPU CLOCK"),
        ("waiting_for_network", "network", "eGPU NET"),
        ("installing", "", "eGPU SETUP"),
        ("installed", "", "eGPU NEXT"),
        ("error", "rejected", "eGPU FILE"),
        ("error", "other", "eGPU ERROR"),
    ] {
        assert_eq!(
            status::delivery_badge(&json!({"state":state,"error_code":error}))
                .unwrap()
                .text,
            text
        );
    }
    assert!(status::fresh(&json!({"updated":7.0001}), 10.));
    assert!(!status::fresh(&json!({"updated":7}), 10.));
    assert!(!status::fresh(&json!({"updated":10.01}), 10.));
    assert!(status::delivery_badge(&json!({"state":"compiled"})).is_none());
}
