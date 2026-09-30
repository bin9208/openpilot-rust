fn main() {
    if std::env::var_os("CARGO_FEATURE_NATIVE").is_none() {
        return;
    }
    cxx_build::bridge("src/bridge.rs")
        .file("native/kernel.cc")
        .include("native")
        .std("c++17")
        .compile("openpilot-sensord-kernel");
    for path in ["src/bridge.rs", "native/kernel.h", "native/kernel.cc"] {
        println!("cargo:rerun-if-changed={path}");
    }
}
