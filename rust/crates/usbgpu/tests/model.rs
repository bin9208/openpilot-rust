use openpilot_usbgpu::model::{status, Paths};
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
