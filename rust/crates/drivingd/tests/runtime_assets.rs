use openpilot_driving_modeld::usb_selection::Configuration;
use std::{
    env,
    path::PathBuf,
    process::Command,
    sync::{atomic::AtomicBool, Arc},
};

#[test]
fn configuration_child() -> Result<(), Box<dyn std::error::Error>> {
    let Some(expected) = env::var_os("EXPECTED_USB_ASSETS") else {
        return Ok(());
    };
    let config = Configuration::for_runtime(Arc::new(AtomicBool::new(false)))?;
    assert_eq!(config.paths.assets, PathBuf::from(expected));
    assert_eq!(
        config.worker,
        env::current_exe()?.with_file_name("openpilot-usbgpu-worker")
    );
    Ok(())
}

#[test]
fn native_entry_preserves_asset_override_and_sibling_default(
) -> Result<(), Box<dyn std::error::Error>> {
    for explicit in [None, Some("/owned/native-assets"), Some("")] {
        let executable = env::current_exe()?;
        let expected =
            explicit.map_or_else(|| executable.with_file_name("usbgpu-assets"), PathBuf::from);
        let mut command = Command::new(executable);
        command
            .args(["--exact", "configuration_child", "--nocapture"])
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
