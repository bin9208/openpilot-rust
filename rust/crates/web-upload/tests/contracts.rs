use openpilot_web_upload::{api_url, normalize_base_url, FolderUpload, Observer};

#[test]
fn api_components_are_encoded_independently() {
    // Given a directory and route containing URL separators.
    let parts = ["upload", "car name/id", "route|0", "rlog.zst"];
    // When building the original service path.
    let actual = api_url("https://example.test/", &parts).unwrap();
    // Then slash and route punctuation cannot become URL structure.
    assert_eq!(
        actual,
        "https://example.test/api/v1/upload/car%20name%2Fid/route%7C0/rlog.zst"
    );
}

#[test]
fn normalization_preserves_source_case_rejection() {
    // Given an uppercase scheme rejected by the source startswith check.
    let input = "HTTPS://example.test";
    // When normalizing the target.
    let result = normalize_base_url(input, "");
    // Then it remains rejected.
    assert!(result.is_err());
}

#[test]
fn explicit_filename_error_precedes_missing_session() {
    // Given no token and a path component the source refuses.
    let folder = tempfile::tempdir().unwrap();
    let filenames = vec!["../secret".to_string()];
    let upload = FolderUpload {
        local_folder: folder.path(),
        directory: "device",
        remote_path: "route",
        base_url: "http://127.0.0.1:1",
        token: "",
        filenames: Some(&filenames),
    };
    // When selecting files before a connection is attempted.
    let result = upload.run(&mut Observer::default());
    // Then the filename failure wins over missing token.
    assert_eq!(result.unwrap_err().to_string(), "invalid upload filename");
}
