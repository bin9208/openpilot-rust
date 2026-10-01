use std::{env, path::PathBuf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if env::var_os("CARGO_FEATURE_NATIVE").is_none() {
        return Ok(());
    }
    let raylib = PathBuf::from(env::var("STARTUP_UI_RAYLIB_ROOT").map_err(|_| {
        "set STARTUP_UI_RAYLIB_ROOT to the locked comma-deps-raylib native install directory"
    })?);
    cxx_build::bridge("src/bridge.rs")
        .file("native/bridge.cc")
        .file("native/raylib_loader.cc")
        .file("native/graphics.cc")
        .file("native/egl.cc")
        .include("native")
        .include(raylib.join("include"))
        .std("c++17")
        .compile("startup-ui-raylib");
    println!("cargo:rerun-if-env-changed=STARTUP_UI_RAYLIB_ROOT");
    println!("cargo:rustc-link-lib=dl");
    for file in [
        "src/bridge.rs",
        "native/bridge.cc",
        "native/bridge.h",
        "native/raylib_loader.cc",
        "native/graphics.cc",
        "native/egl.cc",
        "native/egl.h",
    ] {
        println!("cargo:rerun-if-changed={file}");
    }
    Ok(())
}
