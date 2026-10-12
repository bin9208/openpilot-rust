use openpilot_usbgpu::{
    model::Paths,
    model_delivery::{packaged, pinned_manifest},
};

#[test]
fn missing_packaged_provider_preserves_remote_delivery_and_creates_no_cache() {
    let owned = tempfile::tempdir().unwrap();
    let paths = Paths {
        models: owned.path().join("models"),
        cache: owned.path().join("cache"),
        assets: owned.path().join("assets"),
    };
    assert!(!packaged::seed(&paths, &pinned_manifest()).unwrap());
    assert!(
        packaged::installed(&pinned_manifest(), &paths.cache, &paths.assets)
            .unwrap()
            .is_none()
    );
    assert!(!paths.cache.exists());
}

#[test]
fn unrelated_selected_model_never_reads_or_uses_the_pinned_provider() {
    let owned = tempfile::tempdir().unwrap();
    let paths = Paths {
        models: owned.path().join("models"),
        cache: owned.path().join("cache"),
        assets: owned.path().join("assets"),
    };
    std::fs::create_dir_all(&paths.assets).unwrap();
    std::fs::write(
        paths.assets.join("native-provider.json"),
        b"invalid pinned provider",
    )
    .unwrap();
    let mut selected = pinned_manifest();
    selected.sha256 = "b".repeat(64);
    assert!(!packaged::seed(&paths, &selected).unwrap());
    assert!(packaged::installed(&selected, &paths.cache, &paths.assets)
        .unwrap()
        .is_none());
    selected = pinned_manifest();
    selected.size -= 1;
    assert!(!packaged::seed(&paths, &selected).unwrap());
    assert!(!paths.cache.exists());
}
