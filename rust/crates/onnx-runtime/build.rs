use sha2::{Digest, Sha256};
use std::{env, fs, io, path::PathBuf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-env-changed=OPENPILOT_ORT_ROOT");
    if env::var_os("CARGO_FEATURE_NATIVE_SKIP_MIRI").is_none() {
        return Ok(());
    }
    let root = PathBuf::from(env::var_os("OPENPILOT_ORT_ROOT").ok_or_else(|| {
        io::Error::other("set OPENPILOT_ORT_ROOT to the verified ORT1.23.2 provider")
    })?);
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../upstream/signal-runtime.json"))?;
    let target = env::var("CARGO_CFG_TARGET_ARCH")?;
    let architecture = match target.as_str() {
        "x86_64" => "x64",
        "aarch64" => "aarch64",
        _ => return Err(io::Error::other("unsupported ORT architecture").into()),
    };
    let files = &fixture["onnxruntime"]["providers"][architecture]["files"];
    // Only C declarations are compiled; the provider is validated and dlopened
    // lazily at runtime. No ORT ELF enters a classical process via DT_NEEDED.
    for (name, expected) in files
        .as_object()
        .ok_or_else(|| io::Error::other("missing pinned ORT headers"))?
    {
        if !name.starts_with("include/") {
            continue;
        }
        let bytes = fs::read(root.join(name))?;
        if Some(format!("{:x}", Sha256::digest(&bytes)).as_str()) != expected.as_str() {
            return Err(io::Error::other("ORT header identity mismatch").into());
        }
        println!("cargo:rerun-if-changed={}", root.join(name).display());
    }
    cxx_build::bridge("src/bridge.rs")
        .file("native/session.cc")
        .include("native")
        .include(root.join("include"))
        .std("c++17")
        .compile("openpilot-ort-bridge");
    println!("cargo:rustc-link-lib=dl");
    for file in [
        "src/bridge.rs",
        "native/bridge.h",
        "native/session.cc",
        "../../upstream/signal-runtime.json",
    ] {
        println!("cargo:rerun-if-changed={file}");
    }
    Ok(())
}
