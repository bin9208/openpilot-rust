use openpilot_carrot_server::{egpu_model::ModelFiles, params::Backend};
use openpilot_usbgpu::model::Paths;
use std::{env, fs, path::PathBuf, process::Command};

#[test]
fn asset_configuration_child() -> Result<(), Box<dyn std::error::Error>> {
    let Some(expected) = env::var_os("EXPECTED_USB_ASSETS") else {
        return Ok(());
    };
    let files = ModelFiles::original(std::path::Path::new("/owned/repo"));
    assert_eq!(files.paths.assets, PathBuf::from(expected));
    Ok(())
}

#[test]
fn native_callers_preserve_asset_override_and_installed_default(
) -> Result<(), Box<dyn std::error::Error>> {
    for explicit in [None, Some("/owned/native-assets"), Some("")] {
        let expected = explicit.map_or_else(
            || PathBuf::from("/owned/repo/rust/bin/usbgpu-assets"),
            PathBuf::from,
        );
        let mut command = Command::new(env::current_exe()?);
        command
            .args(["--exact", "asset_configuration_child", "--nocapture"])
            .env("EXPECTED_USB_ASSETS", expected)
            .env_remove("USBGPU_ASSETS_ROOT");
        if let Some(path) = explicit {
            command.env("USBGPU_ASSETS_ROOT", path);
        }
        let output = command.output()?;
        assert!(output.status.success(), "{explicit:?}: {output:?}");
    }
    Ok(())
}

#[test]
fn python_chunk_manifest_does_not_report_native_readiness() -> Result<(), Box<dyn std::error::Error>>
{
    let root = tempfile::tempdir()?;
    let paths = Paths {
        models: root.path().join("models"),
        cache: root.path().join("cache"),
        assets: root.path().join("native-assets"),
    };
    fs::create_dir_all(&paths.models)?;
    fs::create_dir_all(&paths.cache)?;
    let hash = "a".repeat(64);
    fs::write(paths.cache.join("model-aaaaaaaaaaaaaaaa.onnx"), b"abc")?;
    fs::write(
        paths.cache.join("state.json"),
        serde_json::json!({"active":{"model_id":"owned","filename":"model.onnx","size":3,
            "sha256":hash,"url":"https://owned.invalid/model.onnx"},"previous":null})
        .to_string(),
    )?;
    fs::write(
        paths
            .models
            .join("big_driving_aaaaaaaaaaaaaaaa_tinygrad.pkl.chunkmanifest"),
        b"{}",
    )?;
    let params = openpilot_params::Params::open(&root.path().join("params"), "owned")?;
    params.put_bool("UsbGpuHardwareSeen", true)?;
    let backend = Backend::native(params, root.path().join("state"));
    let files = ModelFiles {
        paths,
        usb_devices: root.path().join("sysfs"),
        timestamp: None,
    };
    let response = files.status(&backend)?;
    assert!(!response.get("compiled").truth());
    assert!(response.get("state").text_eq("ready"));
    assert!(response.get("can_restart").truth());
    Ok(())
}
