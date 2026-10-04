use openpilot_pandad::supervisor_runtime::{self, Config};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let library = std::env::var("PANDA_FIRMWARE_USB_LIBRARY")?;
    let api = openpilot_panda_usb::Api::system()?;
    if !std::fs::read_to_string("/proc/self/maps")?.contains(&library) {
        return Err("owned USB fixture missing; refusing device access".into());
    }
    drop(api);
    let mut arguments = std::env::args_os().skip(1);
    let root = arguments.next().ok_or("fixture root missing")?.into();
    let basedir = arguments.next().ok_or("source root missing")?.into();
    let firmware = arguments.next().ok_or("firmware path missing")?.into();
    let child = arguments.next().ok_or("child path missing")?.into();
    let launcher = arguments.next().ok_or("launcher path missing")?.into();
    let cycles = arguments
        .next()
        .and_then(|value| value.to_str().and_then(|text| text.parse::<u64>().ok()))
        .filter(|value| *value > 0)
        .ok_or("positive cycles required")?;
    let owned_path = std::path::PathBuf::from(
        std::env::var_os("PANDA_OWNED_DESCRIPTOR").ok_or("owned descriptor path required")?,
    );
    if !owned_path.is_file() {
        return Err("descriptor fixture must be a regular file".into());
    }
    let pool = openpilot_pandad::firmware::native_spi::Pool::at_path(
        owned_path.to_str().ok_or("owned path UTF-8")?,
    );
    if pool.open(50_000_000, |_, _| Ok(())).is_ok() {
        return Err("regular file unexpectedly configured as SPI".into());
    }
    supervisor_runtime::run(Config {
        root,
        basedir,
        firmware,
        child,
        launcher,
        cycles: Some(cycles),
    })?;
    drop(pool);
    Ok(())
}
