use std::{error::Error, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let path = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?)
        .join("../../../openpilot/common/version.h");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", path.display());
    let version = std::fs::read_to_string(path)?;
    let version = version.split('"').nth(1).ok_or("missing COMMA_VERSION")?;
    println!("cargo:rustc-env=LOGGERD_VERSION={version}");
    Ok(())
}
