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

#[test]
fn md_v3_default_keeps_prior_model_providers_separate() {
    use openpilot_usbgpu::model_delivery::{
        known_manifest, legacy_manifest, retrained_manifest, source_manifest, DEFAULT_MANIFEST_URL,
        LEGACY_MANIFEST_URL, RETRAINED_MANIFEST_URL,
    };
    let mdm = pinned_manifest();
    let cinque = legacy_manifest();
    assert_eq!(
        mdm.sha256,
        "d4c7a3fcfc95813826ca871a80bea18df833aacb2eb6f5333e25a01e65b1a422"
    );
    assert_eq!(mdm.size, 799_941_965);
    assert_eq!(mdm.model_id, "comma-pr39087-md-v3-e86b58dc-d4c7a3fc");
    assert_eq!(
        mdm.cache_filename(),
        "big_driving_tinygrad-d4c7a3fcfc958138.pkl"
    );
    let previous = retrained_manifest();
    assert_eq!(
        previous.sha256,
        "256f2178366479a211933701c03518621ed3970289e6a8a43e5a2bcb15e55b5f"
    );
    assert_eq!(known_manifest(&previous.sha256).unwrap().size, 799_942_179);
    assert_eq!(
        source_manifest(RETRAINED_MANIFEST_URL).unwrap().sha256,
        previous.sha256
    );
    assert_eq!(
        source_manifest(DEFAULT_MANIFEST_URL).unwrap().sha256,
        mdm.sha256
    );
    assert_eq!(
        source_manifest(LEGACY_MANIFEST_URL).unwrap().sha256,
        cinque.sha256
    );
    assert_eq!(known_manifest(&cinque.sha256).unwrap().size, 776_634_338);
    assert!(known_manifest(&"0".repeat(64)).is_none());
    let owned = tempfile::tempdir().unwrap();
    let paths = Paths {
        models: owned.path().join("models"),
        cache: owned.path().join("cache"),
        assets: owned.path().join("assets"),
    };
    std::fs::create_dir_all(&paths.assets).unwrap();
    for previous_model in [cinque, previous] {
        let record = serde_json::json!({"version":1,"kind":"native-generic-hcq", "model_sha256":previous_model.sha256,
      "model_checkpoint":"owned", "pickle":{"path":format!("models/{}/model.pkl",previous_model.sha256),"size":previous_model.size,"sha256":previous_model.sha256},
      "assets_manifest_sha256":"1".repeat(64),"runtime_archive_sha256":"2".repeat(64)});
        std::fs::write(
            paths.assets.join("native-provider.json"),
            serde_json::to_vec(&record).unwrap(),
        )
        .unwrap();
        // A valid legacy provider cannot accidentally seed the new default model.
        assert!(!packaged::seed(&paths, &mdm).unwrap());
        assert!(packaged::ensure(&mdm, &paths.cache, &paths.assets)
            .unwrap()
            .is_none());
    }
    assert!(!paths.cache.exists());
}
