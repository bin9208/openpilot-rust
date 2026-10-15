use sha2::{Digest, Sha256};
use std::{env, fs, io, path::PathBuf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if env::var_os("CARGO_FEATURE_NATIVE_SKIP_MIRI").is_none()
        || env::var("CARGO_CFG_TARGET_ARCH")? != "x86_64"
    {
        return Ok(());
    }
    let source: serde_json::Value = serde_json::from_str(include_str!("vendor/source.json"))?;
    let root = PathBuf::from("vendor/x86-simd-sort");
    for (name, expected) in source["files"]
        .as_object()
        .ok_or_else(|| io::Error::other("numeric source manifest"))?
    {
        let path = root.join(name);
        if Some(format!("{:x}", Sha256::digest(fs::read(&path)?)).as_str()) != expected.as_str() {
            return Err(io::Error::other("numeric source identity mismatch").into());
        }
        println!("cargo:rerun-if-changed={}", path.display());
    }
    cxx_build::bridge("src/bridge.rs")
        .include("native")
        .std("c++17")
        .compile("signal-numeric-bridge");
    let generated =
        PathBuf::from(env::var_os("OUT_DIR").ok_or_else(|| io::Error::other("missing OUT_DIR"))?)
            .join("cxxbridge/include");
    for variant in [3, 4] {
        cc::Build::new()
            .cpp(true)
            .std("c++17")
            .include("native")
            .include(&generated)
            .include(root.join("src"))
            .file(format!("native/sort_v{variant}.cc"))
            .flag(format!("-march=x86-64-v{variant}"))
            .warnings(false)
            .compile(&format!("signal-numeric-v{variant}"));
    }
    for file in [
        "src/bridge.rs",
        "native/bridge.h",
        "native/sort_v3.cc",
        "native/sort_v4.cc",
        "vendor/source.json",
    ] {
        println!("cargo:rerun-if-changed={file}");
    }
    Ok(())
}
