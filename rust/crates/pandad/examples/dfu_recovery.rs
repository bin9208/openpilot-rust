use openpilot_pandad::firmware::{
    client::Environment, native_environment::NativeEnvironment, native_spi::Pool,
};
use serde_json::json;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = std::env::var("PANDA_FIRMWARE_USB_LIBRARY")?;
    let firmware =
        PathBuf::from(std::env::var_os("PANDA_DFU_FIRMWARE").ok_or("firmware path missing")?);
    let owned = firmware.join("owned-regular-spi");
    if !owned.is_file() {
        return Err("regular-file SPI boundary missing".into());
    }
    let api = openpilot_panda_usb::Api::system()?;
    if !std::fs::read_to_string("/proc/self/maps")?.contains(&fixture) {
        return Err("owned libusb fixture missing; refusing device access".into());
    }
    let mut environment = NativeEnvironment::new(
        api,
        Pool::at_path(owned.to_str().ok_or("path UTF-8")?),
        firmware,
        |_, _, _| Ok(()),
    );
    let serial = std::env::var("PANDA_DFU_SERIAL").ok();
    let result = environment.dfu_recover(serial.as_deref());
    eprintln!(
        "DFU_RESULT {}",
        match result {
            Ok(()) => json!({"ok":true}),
            Err(error) => json!({"ok":false,"error":error.to_string()}),
        }
    );
    Ok(())
}
