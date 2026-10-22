#![cfg(feature = "uploader-logging-native")]
use openpilot_dev_tools::uploader_logging_check::{disk_projection, normalized, run, Options};
use serde_json::json;
use std::path::{Path, PathBuf};
#[test]
fn independent_formatter_keeps_array_members_untyped_and_types_dictionary_fields() {
    let input = json!({"msg":{"count":1,"enabled":true,"text":"value","nested":{"ratio":-0.0},"array":[{"count":2}]},"created":2.0});
    assert_eq!(
        disk_projection(&input).unwrap(),
        json!({"msg":{"count$i":1,"enabled$b":true,"text$s":"value","nested":{"ratio$f":-0.0},"array$a":[{"count":2}]},"created":2.0})
    );
}
#[test]
fn uploader_normalization_rejects_negative_speed_and_foreign_http_endpoint() {
    let record = json!({"name":"swaglog","level":"DEBUG","levelnum":10,"msg":{"speed":-1.0}});
    assert!(normalized(&[record], Path::new("/owned"), None).is_err());
    let record = json!({"name":"swaglog","level":"DEBUG","levelnum":10,"msg":"upload_url v1.4 http://example.com/put {}"});
    assert!(normalized(&[record], Path::new("/owned"), None).is_err());
}
#[test]
fn actual_eighteen_cases_cross_both_live_collectors() {
    let path = |key: &str| {
        PathBuf::from(std::env::var_os(key).unwrap_or_else(|| panic!("mandatory {key}")))
    };
    let output = path("UPLOADER_GROUP_PROOF").join("logging");
    let result = run(&Options {
        root: path("OPENPILOT_RUST_ROOT"),
        binary: path("UPLOADER_BINARY"),
        trace: path("UPLOADER_TRACE_BINARY"),
        collector: path("UPLOADER_COLLECTOR_BINARY"),
        worker: path("UPLOADER_LOGGING_CHECKER"),
        fixture: path("UPLOADER_ORIGINAL_LOGGING"),
        fixture_sha256: std::env::var("UPLOADER_ORIGINAL_LOGGING_SHA256")
            .expect("original logging pin"),
        output,
    })
    .unwrap();
    assert_eq!(result["cases"], 18);
    assert_eq!(result["collector_rows"], 36);
}
