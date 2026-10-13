use std::env;
fn main() {
    if env::var_os("CARGO_FEATURE_GUI").is_none() {
        return;
    }
    cxx_build::bridge("src/gui/bridge.rs")
        .file("native/sdl2/gui.cc")
        .include("native")
        .std("c++17")
        .compile("openpilot-sdl2-gui");
    println!("cargo:rustc-link-lib=dl");
    for file in [
        "src/gui/bridge.rs",
        "native/sdl2/gui.cc",
        "native/sdl2/gui.hpp",
        "native/sdl2/abi.hpp",
    ] {
        println!("cargo:rerun-if-changed={file}");
    }
}
