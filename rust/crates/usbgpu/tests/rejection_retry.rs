use openpilot_usbgpu::model_delivery::failure::retry_rejection;
use serde_json::json;
use std::fs;

#[test]
fn retry_is_hash_and_boot_scoped_and_does_not_create_validation() {
    let root = tempfile::tempdir().unwrap();
    let digest = "model-digest";
    let mut failure = json!({"rejected":true,"pickle_sha256":digest,"phase":"inference",
        "boot_id":"previous","error":"RuntimeError: libusb_bulk_transfer: Input/Output Error"});
    let write = |value: &serde_json::Value| {
        fs::write(
            root.path().join("last_failure.json"),
            serde_json::to_vec(value).unwrap(),
        )
        .unwrap()
    };
    write(&failure);
    assert!(retry_rejection(root.path(), digest, "current"));
    assert!(!retry_rejection(root.path(), digest, "previous"));
    assert!(!retry_rejection(root.path(), digest, ""));
    assert!(!retry_rejection(root.path(), "other", "current"));
    assert!(!root.path().join("boot_validation.json").exists());
    for error in [
        "ValueError: corrupted model",
        "libusb_bulk_transfer: Input/Output Error\nValueError: corrupt",
        "AMD failed with 2",
    ] {
        failure["error"] = json!(error);
        write(&failure);
        assert!(!retry_rejection(root.path(), digest, "current"), "{error}");
    }
    for error in [
        "model.NonFiniteModelOutput: bad logits",
        "RepeatedInvalidModelOutput: repeated",
        "ValueError: invalid precompiled model output",
        "non-finite model output",
    ] {
        failure["error"] = json!(error);
        write(&failure);
        assert!(retry_rejection(root.path(), digest, "current"), "{error}");
        failure["phase"] = json!("load");
        write(&failure);
        assert!(!retry_rejection(root.path(), digest, "current"));
        failure["phase"] = json!("inference");
    }
}

#[test]
fn latched_usb_error_requires_pinned_installed_runtime_and_pickle() {
    let root = tempfile::tempdir().unwrap();
    let failure = json!({"rejected":true,"pickle_sha256":"model","phase":"inference",
        "boot_id":"previous","error":"RuntimeError: AMD:0 failed with 1"});
    fs::write(
        root.path().join("last_failure.json"),
        serde_json::to_vec(&failure).unwrap(),
    )
    .unwrap();
    assert!(!retry_rejection(root.path(), "model", "current"));
    let mut installed = json!({"pickle":{"sha256":"model"},"runtime":{"sha256":"90970e01f2ba70d553294c0f9d5c6fe1678827f750705cab679a7b19b7bcb0b2"}});
    fs::write(
        root.path().join("installed.json"),
        serde_json::to_vec(&installed).unwrap(),
    )
    .unwrap();
    assert!(retry_rejection(root.path(), "model", "current"));
    installed["pickle"]["sha256"] = json!("other");
    fs::write(
        root.path().join("installed.json"),
        serde_json::to_vec(&installed).unwrap(),
    )
    .unwrap();
    assert!(!retry_rejection(root.path(), "model", "current"));
}

#[test]
fn serialization_is_explicit_and_persistent_format_is_generic_only() {
    use openpilot_usbgpu::model_delivery::catalog::Catalog;
    let digest = "a".repeat(64);
    let artifact = json!({"url":"model.pkl","size":1,"sha256":digest});
    let mut value = json!({"protocol":1,"format":"comma-generic-onnx","gpu_arch":"gfx1200",
        "model_sha256":digest,"frame_skip":4,"camera_resolutions":[[1928,1208],[1344,760]],
        "pickle":artifact,"runtime":artifact});
    let parse = |value: &serde_json::Value| {
        Catalog::parse(
            &serde_json::to_vec(value).unwrap(),
            &digest,
            "https://models.example/test/precompiled.json",
        )
    };
    assert!(parse(&value).is_ok());
    value["serialization"] = json!("persistent-buffer-v1");
    assert!(parse(&value).is_ok());
    for unsupported in [json!(null), json!("unknown"), json!(2)] {
        value["serialization"] = unsupported;
        assert!(parse(&value).is_err());
    }
    value["serialization"] = json!("persistent-buffer-v1");
    value["format"] = json!("comma-run-model");
    value["onnx_sha256"] = json!(digest);
    assert!(parse(&value).is_err());
}

#[test]
fn native_provider_retry_uses_its_bound_runtime_and_never_overrides_existing_catalog() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("last_failure.json"),
        serde_json::to_vec(
            &json!({"rejected":true,"pickle_sha256":"model","phase":"inference",
      "boot_id":"previous","error":"RuntimeError: AMD failed with -1"}),
        )
        .unwrap(),
    )
    .unwrap();
    fs::write(
        root.path().join("native-provider.json"),
        serde_json::to_vec(&json!({"pickle":{"sha256":"model"},
      "runtime_archive_sha256":"90970e01f2ba70d553294c0f9d5c6fe1678827f750705cab679a7b19b7bcb0b2"}))
        .unwrap(),
    )
    .unwrap();
    assert!(retry_rejection(root.path(), "model", "current"));
    assert!(!retry_rejection(root.path(), "other", "current"));
    fs::write(root.path().join("installed.json"), b"invalid catalog").unwrap();
    assert!(!retry_rejection(root.path(), "model", "current"));
}

#[test]
fn md_v3_previous_boot_usb_failure_requires_matching_pickle_and_verified_runtime() {
    let root = tempfile::tempdir().unwrap();
    let sha = "d4c7a3fcfc95813826ca871a80bea18df833aacb2eb6f5333e25a01e65b1a422";
    let runtime = "c7c58622652231359ecea1a4253b63e157d002242bf596ede1b853f941f937ac";
    let mut installed = json!({"pickle":{"sha256":sha},"runtime":{"sha256":runtime}});
    for phase in ["load", "boot_validation", "inference"] {
        for code in [-1, 1] {
            fs::write(
                root.path().join("installed.json"),
                serde_json::to_vec(&installed).unwrap(),
            )
            .unwrap();
            fs::write(root.path().join("last_failure.json"),serde_json::to_vec(&json!({"rejected":true,"pickle_sha256":sha,"phase":phase,"boot_id":"previous","error":format!("RuntimeError: AMD:0 failed with {code}")})).unwrap()).unwrap();
            assert!(retry_rejection(root.path(), sha, "current"));
            assert!(!retry_rejection(root.path(), sha, "previous"));
            assert!(!retry_rejection(root.path(), "other", "current"));
            installed["runtime"]["sha256"] = json!("unknown");
            fs::write(
                root.path().join("installed.json"),
                serde_json::to_vec(&installed).unwrap(),
            )
            .unwrap();
            assert!(!retry_rejection(root.path(), sha, "current"));
            installed["runtime"]["sha256"] = json!(runtime);
        }
    }
    assert!(!root.path().join("boot_validation.json").exists());
}
