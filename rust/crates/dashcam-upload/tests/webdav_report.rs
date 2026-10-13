use openpilot_dashcam_upload::report::share_text;
use serde_json::json;

#[test]
fn file_storage_reports_paths_without_analysis_links() {
    let report = share_text(&json!({
        "target":"webdav", "remoteBasePath":"routes/Test Car fixture-device/",
        "results":[
            {"segment":"2026-10-13--12-00-00--0","ok":true,"remotePath":"routes/Test Car fixture-device/2026-10-13--12-00-00--0"},
            {"segment":"2026-10-13--12-00-00--1","ok":true,"remotePath":"routes/Test Car fixture-device/2026-10-13--12-00-00--1"}
        ]
    }));
    assert!(report.contains("routes/Test Car fixture-device/"));
    assert!(report.contains("2026-10-13--12-00-00--0 OK"));
    assert!(!report.contains("Open & Analyze"));
    assert!(!report.contains("· Open"));
    assert!(!report.contains("Web/Video/Tools"));
}
