use openpilot_radarcan::settings::{Native, Settings};

#[test]
fn directory_read_error_has_the_original_empty_integer_result() {
    let root = std::env::temp_dir().join(format!(
        "radarcan-params-directory-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let params = openpilot_params::Params::open(&root, "fixture").unwrap();
    std::fs::create_dir(root.join("fixture/EnableRadarTracks")).unwrap();
    let mut settings = Native(params);
    let actual = settings.integer("EnableRadarTracks");
    std::fs::remove_dir_all(root).unwrap();
    assert!(
        matches!(actual, Ok(0)),
        "source directory read yields zero: {actual:?}"
    );
}
