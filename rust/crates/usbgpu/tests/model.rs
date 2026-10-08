use openpilot_usbgpu::model::{remove_active_chunk_manifest, status, Paths};
use serde_json::json;
use std::fs;
#[test]
fn active_model_status_excludes_previous_and_obeys_precompiled_only() {
    let temp = tempfile::tempdir().unwrap();
    let paths = Paths {
        models: temp.path().join("models"),
        cache: temp.path().join("cache"),
    };
    fs::create_dir_all(&paths.models).unwrap();
    fs::create_dir_all(&paths.cache).unwrap();
    assert!(!status(&paths).unwrap().compiled);
    assert!(!status(&paths).unwrap().compile_pending);
    let hash = "a".repeat(64);
    let mut active = json!({"model_id":"test","filename":"model.onnx","size":3,"sha256":hash,"url":"https://models.test/model.onnx"});
    fs::write(paths.cache.join("model-aaaaaaaaaaaaaaaa.onnx"), b"abc").unwrap();
    fs::write(
        paths.cache.join("state.json"),
        json!({"active":active,"previous":null}).to_string(),
    )
    .unwrap();
    assert!(!status(&paths).unwrap().compiled);
    assert!(status(&paths).unwrap().compile_pending);
    fs::write(
        paths
            .models
            .join("big_driving_aaaaaaaaaaaaaaaa_tinygrad.pkl.chunkmanifest"),
        b"1",
    )
    .unwrap();
    assert!(status(&paths).unwrap().compiled);
    assert!(!status(&paths).unwrap().compile_pending);
    active["filename"] = json!("model.pkl");
    fs::write(paths.cache.join("model-aaaaaaaaaaaaaaaa.pkl"), b"abc").unwrap();
    fs::write(
        paths.cache.join("state.json"),
        json!({"active":active,"previous":null}).to_string(),
    )
    .unwrap();
    assert!(!status(&paths).unwrap().compiled);
    assert!(!status(&paths).unwrap().compile_pending);
    fs::write(
        paths.cache.join("state.json"),
        json!({"active":null,"previous":active}).to_string(),
    )
    .unwrap();
    assert!(!status(&paths).unwrap().compiled);
    assert!(!status(&paths).unwrap().compile_pending);
}

#[test]
fn recompile_removes_only_active_chunk_manifest() {
    let temp = tempfile::tempdir().unwrap();
    let paths = Paths {
        models: temp.path().join("models"),
        cache: temp.path().join("cache"),
    };
    fs::create_dir_all(&paths.models).unwrap();
    fs::create_dir_all(&paths.cache).unwrap();
    let active = json!({"model_id":"test","filename":"model.onnx","size":3,
        "sha256":"a".repeat(64),"url":"https://models.test/model.onnx"});
    let previous = paths
        .models
        .join("big_driving_bbbbbbbbbbbbbbbb_tinygrad.pkl.chunkmanifest");
    let compiled = paths
        .models
        .join("big_driving_aaaaaaaaaaaaaaaa_tinygrad.pkl");
    let manifest = paths
        .models
        .join("big_driving_aaaaaaaaaaaaaaaa_tinygrad.pkl.chunkmanifest");
    fs::write(&previous, b"previous").unwrap();
    fs::write(&compiled, b"compiled bytes").unwrap();
    fs::write(&manifest, b"active").unwrap();
    fs::write(paths.cache.join("model-aaaaaaaaaaaaaaaa.onnx"), b"abc").unwrap();
    fs::write(
        paths.cache.join("state.json"),
        json!({"active":null,"previous":active}).to_string(),
    )
    .unwrap();
    assert!(!remove_active_chunk_manifest(&paths).unwrap());
    assert!(manifest.is_file());
    fs::write(
        paths.cache.join("state.json"),
        json!({"active":active,"previous":null}).to_string(),
    )
    .unwrap();
    assert!(remove_active_chunk_manifest(&paths).unwrap());
    assert!(!manifest.exists());
    assert_eq!(fs::read(&compiled).unwrap(), b"compiled bytes");
    assert_eq!(fs::read(&previous).unwrap(), b"previous");
    assert!(!remove_active_chunk_manifest(&paths).unwrap());
}
